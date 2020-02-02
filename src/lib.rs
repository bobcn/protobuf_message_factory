//! A crate to generate a message factory

use glob::glob;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub struct MessageMetadata {
    name: String,
    id: String,
}

/// protobuf message and file info
#[derive(Debug)]
pub struct ProtoMessageInfo {
    file_path: PathBuf,
    file_name: String,
    messages: Vec<MessageMetadata>,
}

/// get protobuf message and file info
pub fn get_protos_info(p: &str) -> Vec<ProtoMessageInfo> {
    let mut v = Vec::new();
    let mut path = p.to_string();

    let msg_id_re = Regex::new(r"//\s*@id:\s*(0[xX][0-9a-fA-F]{4})\s*$").unwrap();
    let msg_name_re = Regex::new(r"message\s+([^\s]+)\s*\{*$").unwrap();

    path.push_str("/*.proto");

    for entry in glob(path.as_str()).expect("Failed to read glob pattern") {
        if let Ok(path) = entry {
            let f = File::open(path.clone()).expect("Failed to open file");
            let reader = BufReader::new(f);
            let mut item = ProtoMessageInfo {
                file_path: path.clone(),
                file_name: path.file_stem().unwrap().to_str().unwrap().to_string(),
                messages: vec![],
            };

            let mut msg_id: Option<String> = None;
            for line in reader.lines() {
                if let None = msg_id {
                    for caps in msg_id_re.captures_iter(line.unwrap().as_str()) {
                        msg_id = Some(caps.get(1).unwrap().as_str().to_string());
                    }
                    continue
                }
                for caps in msg_name_re.captures_iter(line.unwrap().as_str()) {
                    let name = caps.get(1).unwrap().as_str().to_string();
                    item.messages.push(MessageMetadata { name, id: msg_id.unwrap() });
                    msg_id = None;
                }
            }
            v.push(item)
        }
    }

    v
}

/// get proto's filename list
pub fn get_proto_list(v: &Vec<ProtoMessageInfo>) -> Vec<&str> {
    let mut r = Vec::new();
    for f in v.iter() {
        r.push(f.file_path.to_str().unwrap());
    }
    r
}

/// generate factory into `path`
pub fn generate_factory_file(path: &str, v: &Vec<ProtoMessageInfo>) {
    let mut contents = "use std::cell::RefCell;
use std::collections::HashMap;
use protobuf::reflect::MessageDescriptor;
use protobuf::Message;

pub type MessageId = u16;

thread_local! {
    pub static NAME_ID_MAP : RefCell<HashMap<String, MessageId>> = RefCell::new(HashMap::new());
    pub static ID_DESC_MAP : RefCell<HashMap<MessageId, &'static MessageDescriptor>> = RefCell::new(HashMap::new());
}

pub fn register_message<M: Message>(id: MessageId) {
    NAME_ID_MAP.with(|x| {
        let mut m = x.borrow_mut();
        let name = M::descriptor_static().full_name().to_string();
        if !m.contains_key(&name) {
            m.insert(name, id);
        }
    });
    ID_DESC_MAP.with(|x| {
        let mut m = x.borrow_mut();
        let name = M::descriptor_static().full_name().to_string();
        if !m.contains_key(&id) {
            m.insert(id, M::descriptor_static());
        }
    })
}

pub fn get_id(msg_desc: &MessageDescriptor) -> Option<MessageId> {
    NAME_ID_MAP.with(|x| {
        {
            let m = x.borrow_mut();
            if m.len() == 0 {
                drop(m);
                init_descriptors()
            }
        }
        {
            let m = x.borrow_mut();
            match m.get(msg_desc.full_name()) {
                Some(message_id) => Some(message_id.clone()),
                None => None,
            }
        }
    })
}

pub fn get_descriptor(id: MessageId) -> Option<&'static MessageDescriptor> {
    ID_DESC_MAP.with(move |x| {
        {
            let m = x.borrow_mut();
            if m.len() == 0 {
                drop(m);
                init_descriptors()
            }
        }
        {
            let m = x.borrow_mut();
            match m.get(&id) {
                Some(r) => Some(*r),
                None => None,
            }
        }
    })
}".to_string().into_bytes();

    let mut mod_file = File::create((path.to_string() + "/../protos.rs").as_str()).unwrap();
    let mut factory_file = File::create((path.to_string() + "/factory.rs").as_str()).unwrap();

    mod_file.write(b"pub mod factory;\n");

    factory_file.write_all(&contents[..]);
    factory_file.write(b"\n\n");

    let mut crate_path = String::from("crate");
    let parts: Vec<&str> = path.split("/").collect();
    for part in parts[1..].iter() {
        crate_path += format!("::{}", part).as_str();
    }
    for item in v.iter() {
        factory_file.write_fmt(format_args!("use {}::{};\n", crate_path, item.file_name));
        mod_file.write_fmt(format_args!("pub mod {};\n", item.file_name));
    }

    factory_file.write(b"\nfn init_descriptors() {");

    mod_file.write_fmt(format_args!(
        "{}\n{}\n", "pub use factory::get_id;", "pub use factory::get_descriptor;"
    ));

    for file in v.iter() {
        for msg in file.messages.iter() {
            factory_file.write_fmt(format_args!(
                "\n    register_message::<{}::{}>({});",
                file.file_name, msg.name, msg.id
            ));
            mod_file.write_fmt(format_args!(
                "pub use {}::{};\n", file.file_name, msg.name
            ));
        }
    }

    factory_file.write(b"\n}\n");
}
