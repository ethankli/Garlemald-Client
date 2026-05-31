// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

pub const DEFAULT_SERVERS_TOML: &str = include_str!("default_servers.toml");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServerDefinition {
    pub name: String,
    pub address: String,
    pub login_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct ServerDefinitions {
    servers: BTreeMap<String, ServerDefinition>,
}

#[derive(Debug, Deserialize, Default)]
struct ServersFile {
    #[serde(default, rename = "server")]
    servers: Vec<ServerDefinition>,
}

impl ServerDefinitions {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading servers file {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn load_default() -> Result<Self> {
        Self::parse(DEFAULT_SERVERS_TOML)
    }

    pub fn parse(toml_text: &str) -> Result<Self> {
        let parsed: ServersFile = toml::from_str(toml_text).context("parsing servers TOML")?;
        let mut servers: BTreeMap<String, ServerDefinition> = BTreeMap::new();
        for server in parsed.servers {
            if !server.name.is_empty() {
                servers.insert(server.name.clone(), server);
            }
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
        let local = defs.get("Localhost").expect("Localhost present");
        assert_eq!(local.address, "127.0.0.1");
        assert_eq!(local.login_url, "http://127.0.0.1:54993/login");
        let first = defs.iter().next().expect("at least one server");
        assert_eq!(first.name, "Localhost");

        // Project Meteor's PHP login_su runs at 8080, not 54993 (which is
        // the Map Server's game-protocol TCP listener in that codebase).
        let meteor = defs.get("Project Meteor").expect("Project Meteor present");
        assert_eq!(meteor.login_url, "http://127.0.0.1:8080/login.php");
    }

    #[test]
    fn parses_multiple_servers() {
        let toml_text = r#"
[[server]]
name = "A"
address = "a.example"
login_url = "https://a/login"

[[server]]
name = "B"
address = "b.example"
login_url = "https://b/login"
"#;
        let defs = ServerDefinitions::parse(toml_text).unwrap();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs.get("A").unwrap().address, "a.example");
        assert_eq!(defs.get("B").unwrap().login_url, "https://b/login");
    }

    #[test]
    fn empty_document_is_empty_set() {
        let defs = ServerDefinitions::parse("").unwrap();
        assert!(defs.is_empty());
    }
}
