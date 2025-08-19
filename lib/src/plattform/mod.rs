use std::{collections::HashMap, io::Write, path::PathBuf, str::FromStr};
use log::{debug, error, info, warn};

use clap::Parser;

use serde::{Deserialize, Serialize};

use crate::{web::IpMatcher, Config};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

macro_rules! deferred_define {
    ($vis:vis, $name:ident) => {
        #[cfg(target_os = "linux")]
        $vis use linux::$name;

        #[cfg(target_os = "windows")]
        $vis use windows::$name;
    };
    ($name:ident) => {
        #[cfg(target_os = "linux")]
        use linux::$name;

        #[cfg(target_os = "windows")]
        use windows::$name;
    };
}

const MASTER_CONFIG_FILE_NAME: &'static str = "MasterConfig.toml";
const USER_CONFIG_FILE_NAME: &'static str = "Config.toml";

const FOLDER_NAME: &'static str = "DataRace";

deferred_define!(pub(crate), DEFAULT_DASHBOARD_PATH);
deferred_define!(pub(crate), DEFAULT_PLUGIN_SETTINGS_PATH);

deferred_define!(DEFAULT_MASTER_PLUGIN_LOCATIONS);
deferred_define!(DEFAULT_USER_PLUGIN_LOCATIONS);

deferred_define!(get_master_config_path);
deferred_define!(get_config_folder);
deferred_define!(validate_master_config_permissions);
deferred_define!(set_restricted_permissions);

/// Generates the default MasterConfig
/// If write permission to the standard folders is missing, then it is dumped into stdout
///
/// This config needs to be placed into:
/// - Linux: /etc/DataRace/MasterConfig.toml
/// - Windows: [install directory]/MasterConfig.toml
pub(crate) fn default_master_config() {
    fn write_config(config: &String) -> Option<()> {
        let path = get_master_config_path()?;
        let folder = path.parent()?;

        if !folder.is_dir() {
            std::fs::create_dir_all(folder).ok()?;
        }

        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path.as_path()).ok()?;
        file.write_all(config.as_bytes()).ok()?;
        drop(file);

        set_restricted_permissions(&path)?;
        set_restricted_permissions(&folder.to_path_buf())?;

        println!("MastConfig successfully created at {}", path.to_str().unwrap_or_default());

        // Creating default plugins folders
        for item in DEFAULT_MASTER_PLUGIN_LOCATIONS {
            match PathBuf::try_from(PathString(item.to_string())) {
                Ok(path) => {
                    if let Err(e) = std::fs::create_dir_all(path.as_path()) {
                        println!("Unable to create default plugin folder {}: {e}", path.to_str().unwrap_or_default());
                    } else if set_restricted_permissions(&path).is_some() {
                        println!("Created Plugin Folder at {}", path.to_str().unwrap_or_default());
                    } else {
                        println!("Failed to set Permissions for plugin folder created at {}", path.to_str().unwrap_or_default());
                    }
                },
                Err(e) => println!("Was unable to parse default plugin path... This should not be possible: {e}")
            }
        }
        
        Some(())
    }

    let config = MasterConfig { 
        disable_web_server: false, 
        disable_user_plugin_locations: false, 

        web_ip_whitelist: Vec::<String>::new(),
        web_force_ip_whitelist: false,
        web_settings_ip_whitelist: default_settings_ip_whitelist(),
        web_force_settings_ip_whitelist: false,

        plugin_locations: DEFAULT_MASTER_PLUGIN_LOCATIONS.iter().map(|s| PathString(s.to_string())).collect(),
    };

    let output = match toml::to_string_pretty(&config) {
        Ok(out) => out,
        Err(e) => {
            println!("Failed to generate default config (Idk how this could ever happen): {e}");
            return;
        }
    };

    if write_config(&output).is_some() {
        return;
    }

    // Fallback: Dumping it all into stdout
    println!("# Unable to write config, but still generate. Please dump it into the correct location\n{output}");
}

/// This serves to store a path in config, and then convert to a PathBuf.
/// 
/// It converts the following:
/// - ~/ into your home directory
/// - ./ as relative to the current directory
/// - :/ is relative to the install directory
/// - / are converted to \ on Windows
/// - \ are converted to / on Linux
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
struct PathString(String);

impl TryFrom<PathString> for PathBuf {
    type Error = String;

    fn try_from(value: PathString) -> Result<Self, Self::Error> {
        let value = if cfg!(target_os = "windows") {
            value.0.replace("/", "\\").replacen("~\\", "~/", 1).replacen(".\\", "./", 1).replacen(":\\", "./", 1)
        } else {
            value.0.replace("\\", "/")
        };


        if let Some(trimmed) = value.strip_prefix("~/") {
            let mut source = dirs::home_dir().ok_or("Home directory not found".to_string())?;
            source.push(trimmed);

            Ok(source)
        } else if let Some(relative) = value.strip_prefix("./") {
            let mut current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
            current_dir.push(relative);
            
            Ok(current_dir)
        } else if let Some(relative_to_install) = value.strip_prefix(":/") {
            let mut exec_dir = std::env::current_exe().map_err(|e| e.to_string())?;
            exec_dir.pop();
            exec_dir.push(relative_to_install);
            
            Ok(exec_dir)
        } else {
            PathBuf::from_str(value.as_str()).map_err(|e| e.to_string())
        }
    }
}

/// A Master Config serves to force certain paramteres.  
/// 
/// It is supposed to have only root/admin write priviledges to prevent 
/// manipulation from other software.
///
/// It is found in:
/// - Linux: /etc/DataRace/MasterConfig.toml
/// - Windows: [Install-Directory]/MasterConfig.toml
#[derive(Debug, Serialize, Deserialize, Default)]
struct MasterConfig {
    /// Disables the web server when true
    #[serde(default)]
    disable_web_server: bool,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "Vec::new")]
    web_ip_whitelist: Vec<String>,

    #[serde(default)]
    web_force_ip_whitelist: bool,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "Vec::new")]
    web_settings_ip_whitelist: Vec<String>,

    #[serde(default)]
    web_force_settings_ip_whitelist: bool,

    /// When true means only plugin_locations defined in this config will be loaded
    #[serde(default)]
    disable_user_plugin_locations: bool,
    /// Defines the location of plugins. Can be:
    /// - Folders: then files in that folder will be loaded (no traversal into nested folders)
    /// - Files: .so or .dll files of specific plugins to be loaded
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "Vec::new")]
    plugin_locations: Vec<PathString>,
}

fn serde_default_server_ip() -> String {
    crate::web::DEFAULT_IP.to_string()
}
fn serde_default_server_port() -> u16 {
    crate::web::DEFAULT_PORT
}
fn default_settings_ip_whitelist() -> Vec<String> {
    vec!["localhost".to_string()]
}

/// Is the format of the Config.toml
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct UserConfig {
    /// Disables the web server when true
    #[serde(default)]
    disable_web_server: bool,

    #[serde(default = "serde_default_server_ip")]
    web_server_ip: String,
    #[serde(default = "serde_default_server_port")]
    web_server_port: u16,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "Vec::new")]
    web_ip_whitelist: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "default_settings_ip_whitelist")]
    web_settings_ip_whitelist: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default = "Vec::new")]
    plugin_locations: Vec<PathString>,

    dashboards_location: PathString,
    plugin_settings_location: PathString
}

impl Default for UserConfig {
    fn default() -> Self {

        Self {
            disable_web_server: false,
            web_server_ip: serde_default_server_ip(),
            web_server_port: crate::web::DEFAULT_PORT,
            web_ip_whitelist: Vec::new(),
            web_settings_ip_whitelist: default_settings_ip_whitelist(),

            plugin_locations: DEFAULT_USER_PLUGIN_LOCATIONS.iter().map(|s| PathString(s.to_string())).collect(),
            dashboards_location: PathString(DEFAULT_DASHBOARD_PATH.to_string()),
            plugin_settings_location: PathString(DEFAULT_PLUGIN_SETTINGS_PATH.to_string())
        }
    }
}

fn read_master_config() -> Result<MasterConfig, String> {
    let path = get_master_config_path().ok_or("Unable to generate path (this error should be impossible)".to_string())?;


    if !path.is_file() {
        if path.is_dir() {
            return Err(format!("{} Is a folder", path.to_str().unwrap_or_default()));
        } else {
            return Err(format!("{} Does not exist", path.to_str().unwrap_or_default()))
        }
    }

    // Check if write protected
    if !validate_master_config_permissions(&path) {
        return Err("File Permissions insufficiently strict (user should not have write access)".to_string());
    }

    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let res: MasterConfig = toml::from_str(text.as_str()).map_err(|e| e.to_string())?;

    Ok(res)
}

fn read_user_config(disable_user_plugin_locations: bool) -> Result<UserConfig, String> {
    let mut path = get_config_folder().ok_or("Unable to generate path (this error should be impossible)")?;

    if !path.is_dir() {
        if path.is_file() {
            return Err(format!("{} Is a file (needs to be a folder)", path.to_str().unwrap_or_default()));
        } else {
            warn!("User Config Folder does not exist, creating it under: {}", path.to_str().unwrap_or_default());
            std::fs::create_dir_all(path.as_path())
                .map_err(|e| format!("Failed to create folder {}: {}", path.to_str().unwrap_or_default(), e.to_string()))?;
        }
    }
    
    path.push(USER_CONFIG_FILE_NAME);

    if !path.is_file() {
        if path.is_dir() {
            return Err(format!("{} Is a folder", path.to_str().unwrap_or_default()));
        } else {
            // Does not exist, we create it
            warn!("User Config.toml does not exist, creating it under: {}", path.to_str().unwrap_or_default());
            let def = UserConfig::default();
            write_user_config(&def).map_err(|e| format!("Failed to create Config file: {e}"))?;

            if !disable_user_plugin_locations {
                // We create the default plugin locations so they don't throw errors for us

                for path in def.plugin_locations {
                    match PathBuf::try_from(path) {
                        Ok(path) => {
                            if let Err(e) = std::fs::create_dir_all(path.as_path()) {
                                error!("Unable to create default plugin folder {}: {e}", path.to_str().unwrap_or_default());
                            } else {
                                info!("Created Plugin Folder at {}", path.to_str().unwrap_or_default());
                            }
                        },
                        Err(e) => error!("Was unable to parse default plugin path... This should not be possible: {e}")
                    }
                }
            }
        }
    }

    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let res: UserConfig = toml::from_str(text.as_str()).map_err(|e| e.to_string())?;

    Ok(res)
}

pub(crate) fn write_user_config(config: &UserConfig) -> Result<(), String> {
    let mut path = get_config_folder().ok_or("Unable to generate path (this error should be impossible)")?;
    if !path.is_dir() {
        return Err(format!("{} Is not a folder", path.to_str().unwrap_or_default()));
    }
    path.push(USER_CONFIG_FILE_NAME);

    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(path.as_path()).map_err(|e| e.to_string())?;
    let output = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;

    file.write_all(output.as_bytes()).map_err(|e| e.to_string())
}

/// Returns a Config most of the time, even if errors are noted with the boolean being true
pub(crate) fn read_config(init_conf: InitConfig) -> Option<(Config, bool)> {
    debug!("Reading Config...");

    let mut errors = false;

    let master = match read_master_config() {
        Ok(conf) =>  conf,
        Err(e) => {
            error!("Unable to read master config: {}", e);
            // We are currently not failing due to this error
            if cfg!(debug_assertions) {
                errors = true;
                warn!("Continuing with defaults");
                MasterConfig::default()
            } else {
                error!("As a security measure this launch will be terminated (this is disabled on debug builds)");
                return None;
            }
        }
    };

    if master.disable_web_server {
        info!("Web server disabled via MasterConfig");
    }

    let mut user = match read_user_config(master.disable_user_plugin_locations) {
        Ok(conf) => conf,
        Err(e) => {
            error!("Unable to read user config: {}", e);
            warn!("Fallingback to default");
            errors = true;
            UserConfig::default()
        }
    };

    let disable_web_server = master.disable_web_server || user.disable_web_server;

    let config = Config {
        disable_web_server,
        web_server_ip: user.web_server_ip,
        web_server_port: user.web_server_port,
        web_ip_whitelist: create_ip_matcher(disable_web_server, user.web_ip_whitelist, master.web_ip_whitelist, master.web_force_ip_whitelist)?,
        web_settings_whitelist: create_ip_matcher(disable_web_server, user.web_settings_ip_whitelist, master.web_settings_ip_whitelist, master.web_force_settings_ip_whitelist)?,

        plugin_locations: {
            let mut list = master.plugin_locations;

            if master.disable_user_plugin_locations {
                if !user.plugin_locations.is_empty() {
                    warn!("User config defines plugin locations, but MasterConfig has disable_user_plugin_locations=true, ignoring...");
                }
            } else {
                list.append(&mut user.plugin_locations);
            }

            #[cfg(debug_assertions)]
            if init_conf.local_dev {
                // We want our dev enviroment first
                list.push(PathString("./Plugins".to_string()));
                let last = list.len()-1;
                list.swap(0, last);

                warn!("Local Dev is enabled, ./Plugins is added as a Plugins Folder");
            }

            list.into_iter().filter_map(|p| 
                match PathBuf::try_from(p.clone()) {
                    Ok(b) => Some(b),
                    Err(e) => { error!("Skipping plugin location {} due to {}", p.0, e); errors = true; None }
            }).collect()
        },
        dashboards_location: {
            if let Some(over) = init_conf.dashboards_folder {
                create_dashboard_folder(over, &mut errors)?
            } else {
                create_dashboard_folder(user.dashboards_location, &mut errors)?
            }
        },
        plugin_settings_location: {
            if let Some(over) = init_conf.plugin_settings_folder {
                create_plugin_settings_folder(over, &mut errors)?
            } else {
                create_plugin_settings_folder(user.plugin_settings_location, &mut errors)?
            }
        }
    };

    
        
    Some((config, errors))
}

/// Stuffs the boiler plate for error handling and ignoring the list when the sevrer is disabled
///
/// The double Option needs the other escaped as error handling, the inner is part of the type
fn create_ip_matcher(disable_web_server: bool, mut ip_list: Vec<String>, mut master_ip_list: Vec<String>, force_master_config: bool) -> Option<Option<IpMatcher>> {
    if !disable_web_server {
        if force_master_config {
            if !ip_list.is_empty() {
                warn!("MasterConfig has forced it's ip whitelist, user config will be ignored");
            }
            if master_ip_list.is_empty() {
                error!("MasterConfig has forced it's ip whitelist, but it is empty.");
                return None;
            }

            match IpMatcher::new(master_ip_list) {
                Ok(m) => Some(m),
                Err(e) => {
                    error!("Failed to parse ip Whitelist: {}", e);
                    None
                }
            }
        } else {
            ip_list.append(&mut master_ip_list);

            match IpMatcher::new(ip_list) {
                Ok(m) => Some(m),
                Err(e) => {
                    error!("Failed to parse ip Whitelist: {}", e);
                    None
                }
            }
        }
    } else {
        Some(None)
    }
}

fn create_dashboard_folder(path : PathString, errors: &mut bool) -> Option<PathBuf> {
    create_thing_folder(path, errors, "Dashboards", DEFAULT_DASHBOARD_PATH)
}

fn create_plugin_settings_folder(path: PathString, errors: &mut bool) -> Option<PathBuf> {
    create_thing_folder(path, errors, "Plugin Settings", DEFAULT_PLUGIN_SETTINGS_PATH)
}

fn create_thing_folder(path: PathString, errors: &mut bool, thing: &str, fallback_for_thing: &str) -> Option<PathBuf> {
    fn parse_and_create(path: PathString, thing: &str) -> Result<PathBuf, String> {
        let folder = PathBuf::try_from(path.clone()).map_err(|e| e.to_string())?;
        
        if !folder.is_dir() {
            if folder.is_file() {
                return Err("Is File (Must be a Folder)".to_string());
            }
            
            warn!("{thing} Folder does not exist, creating...");
            std::fs::create_dir_all(folder.as_path()).map_err(|e| e.to_string())?;
        }

        return Ok(folder);
    }

    info!("{thing} Folder: {}", path.0.as_str());
    match parse_and_create(path, thing) {
        Ok(path) => return Some(path),
        Err(e) => {
            error!("Failed to access the {thing} folder: {e}");
            *errors = false;
        }
    }

    let fallback = PathString(fallback_for_thing.to_string());
    warn!("Falling back to default {thing} Folder: {}", fallback.0.as_str());
    match parse_and_create(fallback, thing) {
        Ok(path) => Some(path),
        Err(e) => {
            error!("Failed to access fallback {thing} folder: {e}");
            None
        }
    }
}

#[derive(Debug, Parser)]
#[command(about = "Extendable multiplattform Realtime Data processing and visualization Engine for Simracing, Flightsim, Streaming etc.")]
struct CmdArgs {
    /// Makes DataRace also read from local Plugins folder (only available for dev builds)
    #[cfg(debug_assertions)]
    #[arg(long)]
    local_dev: bool,

    /// Generates the master config, will attempt to write it directly, falls back to stdout
    #[arg(long)]
    generate_master_config: bool,

    /// Sets the minimum level to be logged (trace, debug, info, warn, error)
    #[arg(short, long, default_value_t = log::LevelFilter::Info)]
    log_level: log::LevelFilter,

    /// Set (ignoring the value in the Config.toml) the dashboards folder
    #[arg(long, short)]
    dashboards_folder: Option<String>,

    /// Set (ignoring the value in the Config.toml) the plugins settings folder
    #[arg(long, short)]
    plugin_settings_folder: Option<String>,

    /// Print Version Information
    #[arg(short, long)]
    version: bool,
}

/// Initial Config read from the CmdArgs
#[derive(Debug)]
pub(super) struct InitConfig {
    pub(super) log_level: log::LevelFilter,
    dashboards_folder: Option<PathString>,
    plugin_settings_folder: Option<PathString>,

    #[cfg(debug_assertions)]
    local_dev: bool,

}

pub(super) fn read_cmd_args() -> Option<InitConfig> {
    let args = CmdArgs::parse();

    if args.generate_master_config {
        default_master_config();
        return None;
    }

    if args.version {
        println!("DataRace");
        println!("Version: {}.{}.{}", crate::built_info::PKG_VERSION_MAJOR, crate::built_info::PKG_VERSION_MINOR, crate::built_info::PKG_VERSION_PATCH);
        if let Some(commit) = crate::built_info::GIT_COMMIT_HASH_SHORT {
            println!("Commit: {}", commit);
        }
        println!("Plugin API Version: {}", crate::API_VERSION);
        println!("Enviroment: {} - {} - {}", crate::built_info::CFG_OS, crate::built_info::CFG_ENV, crate::built_info::PROFILE);

        return None;
    }

    Some(InitConfig {
        log_level: args.log_level,
        dashboards_folder: args.dashboards_folder.map(|s| PathString(s)),
        plugin_settings_folder: args.plugin_settings_folder.map(|s| PathString(s)),
        
        #[cfg(debug_assertions)]
        local_dev: args.local_dev,

    })

    
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PluginSettingsFile {
    pub(crate) version: [u16; 3],
    pub(crate) settings: HashMap<String, datarace_socket_spec::socket::Value>
}

