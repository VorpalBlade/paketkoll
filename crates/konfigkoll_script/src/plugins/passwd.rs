//! Helpers for working with /etc/passwd and /etc/groups (as well as shadow files)

mod sysusers;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use itertools::Itertools;
use rune::{runtime::Function, Any, ContextError, Module, Value};
use sysusers::{GroupId, UserId};
use winnow::Parser;

use crate::Commands;

use super::package_managers::PackageManager;

type Users = BTreeMap<String, User>;
type Groups = BTreeMap<String, Group>;

/// A representation of the user and group databases
///
/// This can be used to handle `/etc/passwd` and related files.
/// Typically you would:
/// * Create an instance early in the main phase
/// * Add things to it as needed (next to the associated packages)
/// * Apply it at the end of the main phase
///
///
/// A rough example:
///
/// ```rune
/// // Mappings for the IDs that systemd auto-assigns inconsistently from computer to computer
/// const USER_MAPPING = [("systemd-journald", 900), /* ... */]
/// const GROUP_MAPPING = [("systemd-journald", 900), /* ... */]
///
/// pub async fn phase_main(props, cmds, package_managers) {
///     let passwd = passwd::Passwd::new(USER_MAPPING, GROUP_MAPPING);
///
///     let files = ctx.package_managers.get_files();
///     // These two files MUST come first as other files later on refer to them,
///     // and we are not order independent (unlike the real sysusers.d).
///     ctx.passwd.add_from_sysusers(files, "systemd", "/usr/lib/sysusers.d/basic.conf")?;
///     ctx.passwd.add_from_sysusers(files, "filesystem", "/usr/lib/sysusers.d/arch.conf")?;
///
///     // Various other packages and other changes ...
///     ctx.passwd.add_from_sysusers(files, "dbus", "/usr/lib/sysusers.d/dbus.conf")?;
///     // ...
///
///     // Add human user
///     let me = passwd::User::new(1000, "me", "me", "");
///     me.shell = "/bin/zsh";
///     me.home = "/home/me";
///     ctx.passwd.add_user_with_group(me);
///     ctx.passwd.add_user_to_groups("me", ["wheel", "optical", "uucp", "users"]);
///
///     // Don't store passwords in your git repo, load them from the system instead
///     ctx.passwd.passwd_from_system(["me", "root"]);
///
///     // Give root a login shell, we don't want /usr/bin/nologin!
///     ctx.passwd.update_user("root", |user| {
///         user.shell = "/bin/zsh";
///         user
///     });
///
///     // Deal with the IDs not matching (because the mappings were created
///     // before konfigkoll was in use for example)
///     passwd.align_ids_with_system()?;
///
///     // Apply changes
///     passwd.apply(cmds)?;
/// }
/// ```
#[derive(Debug, Any)]
#[rune(item = ::passwd)]
struct Passwd {
    users: Users,
    groups: Groups,
    user_ids: BTreeMap<String, u32>,
    group_ids: BTreeMap<String, u32>,
}

/// Rune API
impl Passwd {
    /// Create a new Passwd instance
    ///
    /// # Arguments
    /// * `user_ids` - A list of tuples of (username, uid) to use if sysusers files does not specify a UID
    /// * `group_ids` - A list of tuples of (groupname, gid) to use if sysusers files does not specify a GID
    #[rune::function(path = Self::new)]
    fn new(user_ids: Vec<(String, u32)>, group_ids: Vec<(String, u32)>) -> Self {
        Self {
            users: BTreeMap::new(),
            groups: BTreeMap::new(),
            user_ids: user_ids.into_iter().collect(),
            group_ids: group_ids.into_iter().collect(),
        }
    }

    /// Add a user to the passwd database
    #[rune::function]
    fn add_user(&mut self, user: User) {
        self.users.insert(user.name.clone(), user);
    }

    /// Add a user to the passwd database (and add a matching group with the same ID)
    #[rune::function]
    fn add_user_with_group(&mut self, user: User) {
        let group = Group {
            name: user.group.clone(),
            gid: user.uid,
            members: Default::default(),
            passwd: "!*".into(),
            admins: Default::default(),
        };
        self.users.insert(user.name.clone(), user);
        self.groups.insert(group.name.clone(), group);
    }

    /// Add a group to the passwd database
    #[rune::function]
    fn add_group(&mut self, group: Group) {
        self.groups.insert(group.name.clone(), group);
    }

    /// Add an already added user to one or more already added groups
    #[rune::function]
    fn add_user_to_groups(&mut self, user: &str, groups: Vec<String>) {
        for group in groups {
            if let Some(group) = self.groups.get_mut(&group) {
                group.members.insert(user.into());
            } else {
                tracing::error!("Group {} not found", group);
            }
        }
    }

    /// Add an already added user to one or more already added groups
    #[rune::function]
    fn add_user_to_groups_as_admin(&mut self, user: &str, groups: Vec<String>) {
        for group in groups {
            if let Some(group) = self.groups.get_mut(&group) {
                group.admins.insert(user.into());
            } else {
                tracing::error!("Group {} not found", group);
            }
        }
    }

    #[rune::function]
    fn update_user(&mut self, user: &str, func: &Function) {
        // TODO: Get rid of expect
        let user = self.users.get_mut(user).expect("User not found");
        *user = func
            .call::<_, User>((user.clone(),))
            .expect("User update call failed");
    }

    #[rune::function]
    fn update_group(&mut self, group: &str, func: &Function) {
        let group = self.groups.get_mut(group).expect("Group not found");
        *group = func
            .call::<_, Group>((group.clone(),))
            .expect("Group update call failed");
    }

    /// Read the passwd and group files from the system and update IDs to match the system (based on name)
    #[rune::function]
    fn align_ids_with_system(&mut self) -> anyhow::Result<()> {
        let passwd = std::fs::read_to_string("/etc/passwd")?;
        for line in passwd.lines() {
            let parts: Vec<_> = line.split(':').collect();
            if parts.len() != 7 {
                tracing::error!("Invalid line in /etc/passwd: {}", line);
                continue;
            }
            let name = parts[0];
            let uid: u32 = parts[2].parse()?;
            if let Some(user) = self.users.get_mut(name) {
                user.uid = uid;
            }
        }

        let group = std::fs::read_to_string("/etc/group")?;
        for line in group.lines() {
            let parts: Vec<_> = line.split(':').collect();
            if parts.len() != 4 {
                tracing::error!("Invalid line in /etc/group: {}", line);
                continue;
            }
            let name = parts[0];
            let gid: u32 = parts[2].parse()?;
            if let Some(group) = self.groups.get_mut(name) {
                group.gid = gid;
            }
        }
        Ok(())
    }

    /// Set user passwords to what they are set to on the system for the given users
    #[rune::function]
    // Allow because rune doesn't work without the owned vec
    #[allow(clippy::needless_pass_by_value)]
    fn passwd_from_system(&mut self, users: Vec<String>) -> anyhow::Result<()> {
        let shadow = std::fs::read_to_string("/etc/shadow")?;
        for line in shadow.lines() {
            let parts: Vec<_> = line.split(':').collect();
            if parts.len() != 9 {
                tracing::error!("Invalid line in /etc/shadow: {}", line);
                continue;
            }
            let name = parts[0];
            let passwd = parts[1];
            if users.contains(&name.to_string()) {
                if let Some(user) = self.users.get_mut(name) {
                    user.passwd = passwd.into();
                }
            }
        }
        Ok(())
    }

    /// Add users and groups declared in a systemd sysusers file
    ///
    /// You need to provide a map of preferred IDs for any IDs not explicitly set in the sysusers file.
    ///
    /// # Arguments
    /// * `package_manager` - The package manager to use for reading the sysusers file
    /// * `config_file` - The path to the sysusers file
    #[rune::function(keep)]
    fn add_from_sysusers(
        &mut self,
        package_manager: &PackageManager,
        package: &str,
        config_file: &str,
    ) -> anyhow::Result<()> {
        let file_contents =
            String::from_utf8(package_manager.file_contents(package, config_file)?)?;
        let parsed = sysusers::parse_file
            .parse(&file_contents)
            .map_err(|error| sysusers::SysusersParseError::from_parse(&error, &file_contents))?;
        for directive in parsed {
            match directive {
                sysusers::Directive::Comment => (),
                sysusers::Directive::User(user) => {
                    let (uid, gid, group) = match user.id {
                        Some(UserId::Uid(uid)) => (uid, None, user.name.clone()),
                        Some(UserId::UidGroup(uid, group)) => (uid, None, group),
                        Some(UserId::UidGid(uid, gid)) => {
                            // Resolve gid to group name
                            let group = self.groups.values().find(|v| v.gid == gid);
                            let group_name = group.map(|g| g.name.as_str()).ok_or_else(|| {
                                anyhow::anyhow!("No group with GID {} for user {}", gid, user.name)
                            })?;
                            (uid, Some(gid), group_name.into())
                        }
                        Some(UserId::FromPath(_)) => {
                            return Err(anyhow::anyhow!("Cannot yet handle user IDs from path"))
                        }
                        None => {
                            let uid = self
                                .user_ids
                                .get(user.name.as_str())
                                .ok_or_else(|| anyhow::anyhow!("No ID for user {}", user.name))?;
                            (*uid, None, user.name.clone())
                        }
                    };
                    self.groups
                        .entry(group.clone().into())
                        .or_insert_with(|| Group {
                            name: group.clone().into(),
                            gid: gid.unwrap_or(uid),
                            members: Default::default(),
                            passwd: "!*".into(),
                            admins: Default::default(),
                        });
                    self.users
                        .entry(user.name.clone().into_string())
                        .or_insert_with(|| User {
                            uid,
                            name: user.name.into_string(),
                            group: group.into(),
                            gecos: user.gecos.map(Into::into).unwrap_or_default(),
                            home: user.home.map(Into::into).unwrap_or_else(|| "/".into()),
                            shell: user
                                .shell
                                .map(Into::into)
                                .unwrap_or_else(|| "/usr/bin/nologin".into()),
                            passwd: "!*".into(),
                            change: None,
                            min: None,
                            max: None,
                            warn: None,
                            inact: None,
                            expire: None,
                        });
                }
                sysusers::Directive::Group(group) => {
                    let gid = match group.id {
                        Some(GroupId::Gid(gid)) => gid,
                        Some(GroupId::FromPath(_)) => {
                            return Err(anyhow::anyhow!("Cannot yet handle group IDs from path"))
                        }
                        None => self
                            .group_ids
                            .get(group.name.as_str())
                            .copied()
                            .ok_or_else(|| anyhow::anyhow!("No ID for group {}", group.name))?,
                    };
                    self.groups
                        .entry(group.name.clone().into_string())
                        .or_insert_with(|| Group {
                            name: group.name.into_string(),
                            gid,
                            members: Default::default(),
                            passwd: "!*".into(),
                            admins: Default::default(),
                        });
                }
                sysusers::Directive::AddUserToGroup { user, group } => {
                    if let Some(group) = self.groups.get_mut(group.as_str()) {
                        group.members.insert(user.into_string());
                    } else {
                        tracing::error!("Group {} not found", group);
                    }
                }
                sysusers::Directive::SetRange(_, _) => (),
            }
        }
        Ok(())
    }

    /// Apply to commands
    #[rune::function]
    fn apply(self, cmds: &mut Commands) -> anyhow::Result<()> {
        let mut passwd = String::new();
        let mut shadow = String::new();
        let users = self.users.values().sorted().collect_vec();
        let groups = self.groups.values().sorted().collect_vec();
        for user in users {
            writeln!(passwd, "{}", user.format_passwd(&self.groups))?;
            writeln!(shadow, "{}", user.format_shadow())?;
        }
        let mut groups_contents = String::new();
        let mut gshadow = String::new();
        for group in groups {
            writeln!(groups_contents, "{}", group.format_group())?;
            writeln!(gshadow, "{}", group.format_gshadow())?;
        }
        for suffix in ["", "-"] {
            cmds.write(&format!("/etc/passwd{suffix}"), passwd.as_bytes())?;
            cmds.write(&format!("/etc/group{suffix}"), groups_contents.as_bytes())?;
            let shadow_file = format!("/etc/shadow{suffix}");
            cmds.write(&shadow_file, shadow.as_bytes())?;
            let gshadow_file = format!("/etc/gshadow{suffix}");
            cmds.write(&gshadow_file, gshadow.as_bytes())?;
            if suffix == "-" {
                // This is already set by package management for the main files
                cmds.chmod(&shadow_file, Value::Integer(0o600))?;
                cmds.chmod(&gshadow_file, Value::Integer(0o600))?;
            }
        }
        Ok(())
    }
}

/// Represents a user
#[derive(Any, Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[rune(item = ::passwd)]
struct User {
    // passwd info
    /// User ID
    #[rune(get, set)]
    uid: u32,
    /// Username
    #[rune(get, set)]
    name: String,
    /// Group name
    #[rune(get, set)]
    group: String,
    /// User information
    #[rune(get, set)]
    gecos: String,
    /// Home directory
    #[rune(get, set)]
    home: String,
    /// Path to shell
    #[rune(get, set)]
    shell: String,

    // Shadow info
    /// User password (probably hashed)
    #[rune(get, set)]
    passwd: String,

    /// Last password change (days since epoch)
    #[rune(get, set)]
    change: Option<u64>,
    /// Min password age (days)
    #[rune(get, set)]
    min: Option<u32>,
    /// Max password age (days)
    #[rune(get, set)]
    max: Option<u32>,
    /// Password warning period (days)
    #[rune(get, set)]
    warn: Option<u32>,
    /// Password inactivity period (days)
    #[rune(get, set)]
    inact: Option<u32>,
    /// Account expiration date (days since epoch)
    #[rune(get, set)]
    expire: Option<u64>,
}

/// Rust API
impl User {
    fn format_passwd(&self, groups: &Groups) -> String {
        format!(
            "{name}:x:{uid}:{gid}:{gecos}:{dir}:{shell}",
            name = self.name,
            uid = self.uid,
            gid = groups.get(&self.group).map(|g| g.gid).unwrap_or(0),
            gecos = self.gecos,
            dir = self.home,
            shell = self.shell,
        )
    }

    fn format_shadow(&self) -> String {
        let f64 = |v: Option<u64>| v.map(|v| format!("{v}")).unwrap_or("".into());
        let f32 = |v: Option<u32>| v.map(|v| format!("{v}")).unwrap_or("".into());
        format!(
            "{name}:{passwd}:{change}:{min}:{max}:{warn}:{inact}:{expire}:",
            name = self.name,
            passwd = self.passwd,
            change = f64(self.change),
            min = f32(self.min),
            max = f32(self.max),
            warn = f32(self.warn),
            inact = f32(self.inact),
            expire = f64(self.expire),
        )
    }
}

/// Rune API
impl User {
    /// Create a new User
    ///
    /// This is optimised for a system user with sensible defaults.
    ///
    /// These defaults are:
    /// * Home directory: `/`
    /// * Shell: `/usr/bin/nologin`
    /// * Password: `!*` (no login)
    /// * No password expiration/age/warning/etc
    /// * No account expiration
    #[rune::function(path = Self::new)]
    fn new(uid: u32, name: String, group: String, gecos: String) -> Self {
        Self {
            uid,
            name,
            group,
            gecos,
            home: "/".into(),
            shell: "/usr/bin/nologin".into(),
            passwd: "!*".into(),
            change: None,
            min: None,
            max: None,
            warn: None,
            inact: None,
            expire: None,
        }
    }
}

/// Represents a group
#[derive(Any, Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[rune(item = ::passwd)]
struct Group {
    /// Group ID
    #[rune(get, set)]
    gid: u32,
    /// Group name
    #[rune(get, set)]
    name: String,
    /// Group members
    members: BTreeSet<String>,

    // Shadow info
    /// Password for group (probably hashed)
    #[rune(get, set)]
    passwd: String,
    // Administrators
    admins: BTreeSet<String>,
}

/// Rust API
impl Group {
    fn format_group(&self) -> String {
        let members = self
            .members
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",");
        format!("{name}:x:{gid}:{members}", name = self.name, gid = self.gid,)
    }

    fn format_gshadow(&self) -> String {
        let members = self
            .members
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let admins = self
            .admins
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{name}:{passwd}:{admins}:{members}",
            name = self.name,
            passwd = self.passwd,
            members = members,
            admins = admins,
        )
    }
}

/// Rune API
impl Group {
    /// Create a new group
    #[rune::function(path = Self::new)]
    fn new(name: String, gid: u32) -> Self {
        Self {
            name,
            gid,
            members: BTreeSet::new(),
            passwd: "!*".into(),
            admins: BTreeSet::new(),
        }
    }
}

#[rune::module(::passwd)]
/// Utilities for patching file contents conveniently.
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Passwd>()?;
    m.ty::<User>()?;
    m.ty::<Group>()?;

    m.function_meta(Passwd::new)?;
    m.function_meta(Passwd::add_user)?;
    m.function_meta(Passwd::add_group)?;
    m.function_meta(Passwd::add_user_with_group)?;
    m.function_meta(Passwd::add_user_to_groups)?;
    m.function_meta(Passwd::add_user_to_groups_as_admin)?;
    m.function_meta(Passwd::add_from_sysusers__meta)?;
    m.function_meta(Passwd::passwd_from_system)?;
    m.function_meta(Passwd::align_ids_with_system)?;
    m.function_meta(Passwd::update_group)?;
    m.function_meta(Passwd::update_user)?;
    m.function_meta(Passwd::apply)?;
    m.function_meta(User::new)?;
    m.function_meta(Group::new)?;

    Ok(m)
}