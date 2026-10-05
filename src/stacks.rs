//! Built-in stacks: apps that belong together, installed as one.
//!
//! A stack is what makes "app + dependency" a single command. Its members are
//! installed on the stack's own network, where they reach each other by the
//! name they have *in the stack* — `db`, not `erst-app-db` — so the
//! environment a stack wires up reads the way the apps' own documentation
//! writes it.

use anyhow::Result;

use crate::app::AppSettings;
use crate::catalog;

/// Apps that belong together and are installed as one.
#[derive(Debug, PartialEq, Eq)]
pub struct Stack {
    /// What the stack is called, and the name of the network its members share.
    pub name: &'static str,
    /// What you end up with.
    pub description: &'static str,
    /// Members in install order: the dependency goes first, because everything
    /// after it starts out trying to connect to something that is not up yet.
    pub members: &'static [Member],
}

/// One app inside a stack.
#[derive(Debug, PartialEq, Eq)]
pub struct Member {
    /// What the app is called here. This is also the name it answers to on
    /// the stack's network, which is what the environment below refers to —
    /// and since app names are global, no other stack may use the same one.
    pub name: &'static str,
    /// The image to install, tag and all: a stack says exactly what it needs
    /// rather than hoping the catalog keeps the same tag.
    pub image: &'static str,
    /// Ports published to your machine, as `HOST:CONTAINER` or `CONTAINER`.
    ///
    /// Empty is the right answer for a dependency: the database is reachable
    /// from inside the stack and from nowhere else, and two stacks can then
    /// both bring their own copy without fighting over port 3306.
    pub ports: &'static [&'static str],
    /// Environment for the app. Values name other members directly, because
    /// that is what the shared network makes true.
    pub env: &'static [(&'static str, &'static str)],
}

pub const STACKS: &[Stack] = &[
    Stack {
        name: "blog",
        description: "A self-hosted blog",
        members: &[
            Member {
                name: "blog-db",
                image: "mariadb:lts",
                ports: &[],
                env: &[
                    ("MYSQL_ROOT_PASSWORD", "change-me"),
                    ("MYSQL_DATABASE", "wordpress"),
                    ("MYSQL_USER", "wordpress"),
                    ("MYSQL_PASSWORD", "change-me"),
                ],
            },
            Member {
                name: "wordpress",
                image: "wordpress:apache",
                ports: &["8080:80"],
                env: &[
                    ("WORDPRESS_DB_HOST", "blog-db"),
                    ("WORDPRESS_DB_USER", "wordpress"),
                    ("WORDPRESS_DB_PASSWORD", "change-me"),
                    ("WORDPRESS_DB_NAME", "wordpress"),
                ],
            },
        ],
    },
    Stack {
        name: "ghost",
        description: "Ghost, a newsletter or blog of your own",
        members: &[
            Member {
                name: "ghost-db",
                image: "mariadb:lts",
                ports: &[],
                env: &[
                    ("MYSQL_ROOT_PASSWORD", "change-me"),
                    ("MYSQL_DATABASE", "ghost"),
                    ("MYSQL_USER", "ghost"),
                    ("MYSQL_PASSWORD", "change-me"),
                ],
            },
            Member {
                name: "ghost",
                image: "ghost:5-alpine",
                ports: &["2368:2368"],
                env: &[
                    ("NODE_ENV", "production"),
                    ("url", "http://localhost:2368"),
                    ("database__connection__client", "mysql"),
                    ("database__connection__host", "ghost-db"),
                    ("database__connection__port", "3306"),
                    ("database__connection__user", "ghost"),
                    ("database__connection__password", "change-me"),
                    ("database__connection__database", "ghost"),
                ],
            },
        ],
    },
];

/// Find a stack by name.
pub fn find(name: &str) -> Option<&'static Stack> {
    STACKS.iter().find(|stack| stack.name == name)
}

/// Every stack, in the order `erst stacks` lists them.
pub fn entries() -> &'static [Stack] {
    STACKS
}

impl Stack {
    /// The names of the apps in this stack, in install order.
    pub fn member_names(&self) -> Vec<&'static str> {
        self.members.iter().map(|member| member.name).collect()
    }
}

impl Member {
    /// The settings this member installs with, on the stack's network.
    pub fn settings(&self, stack: &Stack) -> Result<AppSettings> {
        let ports: Vec<String> = self.ports.iter().map(|port| port.to_string()).collect();
        let env: Vec<String> = self
            .env
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();

        // The image is all the catalog would have contributed here: ports and
        // environment are the stack's to decide, down to publishing nothing.
        let mut settings = catalog::image_settings(self.image, &ports, &env)?;
        settings.name = self.name.to_string();
        settings.network = Some(stack.name.to_string());
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::{STACKS, entries, find};
    use std::collections::HashSet;

    #[test]
    fn stacks_are_named_so_their_networks_do_not_collide() {
        let names: HashSet<_> = STACKS.iter().map(|stack| stack.name).collect();
        assert_eq!(names.len(), STACKS.len());
        for stack in entries() {
            assert_eq!(find(stack.name), Some(stack));
        }
        assert!(find("nope").is_none());
    }

    #[test]
    fn every_stack_installs_its_database_first() {
        for stack in entries() {
            let names: Vec<_> = stack.members.iter().map(|member| member.name).collect();
            let unique: HashSet<_> = names.iter().copied().collect();
            assert_eq!(names.len(), unique.len(), "{} repeats a member", stack.name);
            assert_eq!(
                names.first().map(|name| name.ends_with("-db")),
                Some(true),
                "{} should install its database first",
                stack.name
            );
        }
    }

    /// App names are global, not per stack: `db` in two stacks would mean the
    /// second one cannot be installed once the first is, and `erst stacks`
    /// would count a database it does not own.
    #[test]
    fn no_two_stacks_ask_for_the_same_app_name() {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for stack in entries() {
            for member in stack.members {
                assert!(
                    seen.insert(member.name),
                    "{} wants `{}`, which another stack already uses",
                    stack.name,
                    member.name
                );
            }
        }
    }

    #[test]
    fn a_stack_member_does_not_shoot_a_catalog_app() {
        for stack in entries() {
            for member in stack.members {
                assert!(
                    crate::catalog::find(member.name).is_none(),
                    "{} wants `{}`, which is a catalog app you might already \
                     have installed for other reasons",
                    stack.name,
                    member.name
                );
            }
        }
    }

    #[test]
    fn members_come_up_on_the_stack_network_under_their_stack_name() {
        let stack = find("blog").expect("the blog stack exists");
        let wordpress = stack.members[1].settings(stack).expect("settings build");

        assert_eq!(wordpress.name, "wordpress");
        assert_eq!(wordpress.image, "wordpress:apache");
        assert_eq!(wordpress.network.as_deref(), Some("blog"));
        assert_eq!(wordpress.ports.len(), 1);
        assert_eq!(wordpress.ports[0].host, 8080);
        assert_eq!(wordpress.ports[0].container, 80);
        assert_eq!(
            wordpress.env.get("WORDPRESS_DB_HOST").map(String::as_str),
            Some("blog-db"),
            "the connection points at the member's name, which is also its alias"
        );
    }

    #[test]
    fn a_dependency_is_reachable_only_from_inside_the_stack() {
        for stack in entries() {
            let database = &stack.members[0];
            let settings = database.settings(stack).expect("settings build");

            assert!(
                settings.ports.is_empty(),
                "{} publishes its database to the machine: it should only be \
                 reachable from the stack",
                stack.name
            );
        }
    }

    #[test]
    fn a_stack_does_not_publish_one_port_twice() {
        for stack in entries() {
            let hosts: Vec<u16> = stack
                .members
                .iter()
                .flat_map(|member| member.settings(stack).expect("settings build").ports)
                .map(|port| port.host)
                .collect();
            let unique: HashSet<_> = hosts.iter().copied().collect();

            assert_eq!(
                hosts.len(),
                unique.len(),
                "{} would ask for two apps on the same host port",
                stack.name
            );
        }
    }

    #[test]
    fn a_stack_only_connects_to_the_members_it_installs() {
        for stack in entries() {
            let names: HashSet<_> = stack.member_names().into_iter().collect();

            for member in stack.members {
                let settings = member.settings(stack).expect("settings build");
                assert_eq!(
                    settings.network.as_deref(),
                    Some(stack.name),
                    "{} does not join its own network",
                    member.name
                );

                // The alias a connection setting names only resolves for the
                // apps this stack installs, so anything pointing at an app has
                // to point at one of them.
                for (key, value) in &settings.env {
                    if !key.to_ascii_lowercase().contains("host") {
                        continue;
                    }
                    assert!(
                        names.contains(value.as_str()),
                        "{}:{} sets {key} to `{value}`, which is not a member of {}",
                        stack.name,
                        member.name,
                        stack.name
                    );
                }
            }
        }
    }
}
