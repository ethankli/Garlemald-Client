# NOTICE

Garlemald Client is part of a Rust port of a FINAL FANTASY XIV v1.23b
server and client emulator. The port was made possible by, and derives
directly from, the following upstream projects:

## Project Meteor Server

- Source: <https://bitbucket.org/Ioncannon/project-meteor-server>
- License: GNU Affero General Public License v3.0 (AGPL-3.0)

Project Meteor Server is the C# FFXIV 1.23b server emulator
(Lobby / World / Map) maintained by Ioncannon and contributors. The
sibling `garlemald-server` project is a Rust reimplementation of its
protocol, data model, and Lua-driven content systems, and
`garlemald-client` speaks the same wire protocol its lobby expects.

Credit for the reverse-engineering of the 1.x wire protocol, packet
headers, opcodes, Blowfish session handshake, ZiPatch format, actor
system, quest/Director state machines, and the overall server
architecture belongs to the Project Meteor Server authors.

## Seventh Umbral

- Source: <https://github.com/Meteor-Project/SeventhUmbral>
- License: 2-clause BSD-style (see upstream `License.txt`)

Seventh Umbral is the original Windows-only C++ launcher, renderer,
and client-side research suite used to drive the 1.23b client against
Project Meteor Server. `garlemald-client` is a cross-platform Rust
rewrite of its launcher component — game-install detection, patch
download and apply, login handshake, and server-address injection into
the game binary. The launcher's behavior, PE-patching approach, and
1.x client quirks documented in Seventh Umbral's source are the basis
for the corresponding logic in this crate.

## Acknowledgments

Thanks to Ioncannon, Jean-Philip Desjardins, and every contributor to
Project Meteor Server, Seventh Umbral, the FFXIV Classic wiki
(<http://ffxivclassic.fragmenterworks.com/wiki/>), and the wider
community of 1.0 preservationists whose notes, spreadsheets, and
packet captures made this port feasible.

Any bugs in the Rust port are the fault of this project, not of the
upstream work it builds on.
