use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

pub const DEFAULT_SERVERS_XML: &str = include_str!("default_servers.xml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDefinition {
    pub name: String,
    pub address: String,
    pub login_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct ServerDefinitions {
    servers: BTreeMap<String, ServerDefinition>,
}

impl ServerDefinitions {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading servers file {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn load_default() -> Result<Self> {
        Self::parse(DEFAULT_SERVERS_XML)
    }

    pub fn parse(xml: &str) -> Result<Self> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut servers: BTreeMap<String, ServerDefinition> = BTreeMap::new();
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Eof => break,
                Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"Server" => {
                    let mut name = String::new();
                    let mut address = String::new();
                    let mut login_url = String::new();
                    for attr in e.attributes() {
                        let attr = attr?;
                        let value = attr.unescape_value()?.into_owned();
                        match attr.key.as_ref() {
                            b"Name" => name = value,
                            b"Address" => address = value,
                            b"LoginUrl" => login_url = value,
                            _ => {}
                        }
                    }
                    if !name.is_empty() {
                        servers.insert(
                            name.clone(),
                            ServerDefinition { name, address, login_url },
                        );
                    }
                }
                _ => {}
            }
            buf.clear();
        }
        Ok(Self { servers })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ServerDefinition> {
        self.servers.values()
    }

    pub fn get(&self, name: &str) -> Option<&ServerDefinition> {
        self.servers.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.servers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_servers() {
        let defs = ServerDefinitions::load_default().unwrap();
        let van = defs.get("Van Darnus Server").expect("Van Darnus present");
        assert_eq!(van.address, "vandarnus.seventhumbral.org");
        assert_eq!(van.login_url, "https://vandarnus.seventhumbral.org/login.php");
    }

    #[test]
    fn parses_multiple_servers() {
        let xml = r#"<Servers>
            <Server Name="A" Address="a.example" LoginUrl="https://a/login" />
            <Server Name="B" Address="b.example" LoginUrl="https://b/login" />
        </Servers>"#;
        let defs = ServerDefinitions::parse(xml).unwrap();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs.get("A").unwrap().address, "a.example");
        assert_eq!(defs.get("B").unwrap().login_url, "https://b/login");
    }

    #[test]
    fn empty_document_is_empty_set() {
        let defs = ServerDefinitions::parse("<Servers></Servers>").unwrap();
        assert!(defs.is_empty());
    }
}
