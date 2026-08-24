//! Remote hosts that a workspace can run on.
//!
//! A remote workspace's panes live in a wezmux mux server on another host,
//! reached over ssh. Hosts come from the `ssh_domains` config; the ones used
//! before are also remembered here so they can be offered again.

use config::{ConfigHandle, SshDomain, SshMultiplexing, SshParameters};
use mux::domain::Domain;
use mux::Mux;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use wezterm_client::domain::{ClientDomain, ClientDomainConfig};

/// How many past sessions to remember.
const MAX_REMEMBERED: usize = 20;

/// Where a workspace should run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Local,
    /// A new workspace on `host`.
    NewRemote { host: String },
    /// A workspace that already exists, or existed, on `host`.
    Remote { host: String, workspace: String },
    /// Ask for a host that isn't offered yet.
    PromptForHost,
}

/// A host and workspace used before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedSession {
    pub host: String,
    pub workspace: String,
    /// RFC 3339, for display and ordering.
    pub last_used: String,
}

/// One row of the new-workspace picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub label: String,
    /// Where the workspace runs, or when it was last used.
    pub detail: String,
    pub target: WorkspaceTarget,
}

/// What the picker needs to know about the current state of the world.
#[derive(Debug, Clone, Default)]
pub struct PickerContext {
    /// Workspaces that exist right now, remote ones included.
    pub live_workspaces: Vec<String>,
    /// Hosts whose domain is attached.
    pub attached_hosts: Vec<String>,
    /// Hosts named explicitly in the config.
    pub configured_hosts: Vec<String>,
}

/// Build the picker rows: local first, then the remembered sessions in recency
/// order, then a new workspace per known host, then an escape hatch for a host
/// that isn't listed.
pub fn build_picker_entries(sessions: &RemoteSessions, ctx: &PickerContext) -> Vec<PickerEntry> {
    let mut entries = vec![PickerEntry {
        label: "Local".to_string(),
        detail: "this machine".to_string(),
        target: WorkspaceTarget::Local,
    }];

    for session in &sessions.sessions {
        let running = ctx.attached_hosts.contains(&session.host)
            && ctx.live_workspaces.contains(&session.workspace);

        entries.push(PickerEntry {
            label: format!("{} · {}", session.workspace, session.host),
            detail: if running {
                "running".to_string()
            } else {
                describe_last_used(&session.last_used)
            },
            target: WorkspaceTarget::Remote {
                host: session.host.clone(),
                workspace: session.workspace.clone(),
            },
        });
    }

    let mut hosts = sessions.hosts();
    for host in &ctx.configured_hosts {
        if !hosts.contains(host) {
            hosts.push(host.clone());
        }
    }
    for host in hosts {
        entries.push(PickerEntry {
            label: format!("New workspace on {host}"),
            detail: "remote".to_string(),
            target: WorkspaceTarget::NewRemote { host },
        });
    }

    entries.push(PickerEntry {
        label: "Other host…".to_string(),
        detail: "remote".to_string(),
        target: WorkspaceTarget::PromptForHost,
    });

    entries
}

/// Render an RFC 3339 timestamp as a rough age, e.g. "3h ago".
fn describe_last_used(timestamp: &str) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
        return "earlier".to_string();
    };
    let age = chrono::Utc::now().signed_duration_since(parsed);

    if age.num_minutes() < 1 {
        "just now".to_string()
    } else if age.num_hours() < 1 {
        format!("{}m ago", age.num_minutes())
    } else if age.num_days() < 1 {
        format!("{}h ago", age.num_hours())
    } else if age.num_days() < 30 {
        format!("{}d ago", age.num_days())
    } else {
        parsed.format("%Y-%m-%d").to_string()
    }
}

/// Hosts and workspaces used before, most recent first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteSessions {
    #[serde(default)]
    pub sessions: Vec<RememberedSession>,
}

impl RemoteSessions {
    fn path() -> PathBuf {
        config::HOME_DIR
            .join(".config")
            .join("wezmux")
            .join("remote-sessions.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!("ignoring {}: {err:#}", path.display());
                Self::default()
            }),
            Err(err) => {
                log::warn!("could not read {}: {err:#}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        let dir = path.parent().expect("path always has a parent");
        std::fs::create_dir_all(dir)?;

        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serde_json::to_string_pretty(self)?.as_bytes())?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Move `host`/`workspace` to the front, dropping the oldest entries.
    pub fn record(&mut self, host: &str, workspace: &str, now: String) {
        self.sessions
            .retain(|s| !(s.host == host && s.workspace == workspace));
        self.sessions.insert(
            0,
            RememberedSession {
                host: host.to_string(),
                workspace: workspace.to_string(),
                last_used: now,
            },
        );
        self.sessions.truncate(MAX_REMEMBERED);
    }

    pub fn forget_host(&mut self, host: &str) {
        self.sessions.retain(|s| s.host != host);
    }

    /// The hosts appearing in the history, most recently used first.
    pub fn hosts(&self) -> Vec<String> {
        let mut hosts = vec![];
        for session in &self.sessions {
            if !hosts.contains(&session.host) {
                hosts.push(session.host.clone());
            }
        }
        hosts
    }
}

/// The configured domains that run a mux server over ssh.
pub fn configured_remote_domains(config: &ConfigHandle) -> Vec<SshDomain> {
    config
        .ssh_domains()
        .into_iter()
        .filter(|dom| dom.multiplexing == SshMultiplexing::WezTerm)
        .collect()
}

/// Hosts named explicitly in the config, as opposed to those wezterm
/// synthesises from `~/.ssh/config`.
pub fn explicitly_configured_hosts(config: &ConfigHandle) -> Vec<String> {
    if config.ssh_domains.is_none() {
        return vec![];
    }
    configured_remote_domains(config)
        .into_iter()
        .map(|dom| dom.remote_address)
        .collect()
}

/// Find the configured domain for `host`, matching the domain name or the
/// remote address, with or without a port.
pub fn find_configured(configured: Vec<SshDomain>, host: &str) -> Option<SshDomain> {
    let host_only = host.split(':').next().unwrap_or(host);
    configured.into_iter().find(|dom| {
        let addr_host = dom
            .remote_address
            .split(':')
            .next()
            .unwrap_or(&dom.remote_address);
        dom.name == host || addr_host == host_only
    })
}

/// Build the ssh domain for `host`: the configured one if there is any, else an
/// ad-hoc `SSHMUX:host`. The remote wezmux path is resolved on connect when the
/// config does not name one.
pub fn domain_config_for_host(config: &ConfigHandle, host: &str) -> anyhow::Result<SshDomain> {
    if let Some(dom) = find_configured(configured_remote_domains(config), host) {
        return Ok(dom);
    }

    let params = SshParameters::from_str(host)?;
    Ok(SshDomain {
        name: format!("SSHMUX:{host}"),
        remote_address: params.host_and_port,
        username: params.username,
        multiplexing: SshMultiplexing::WezTerm,
        // Derived Default leaves this None, which turns off predictive echo and
        // makes every keystroke wait for a round trip.
        local_echo_threshold_ms: config::default_local_echo_threshold_ms(),
        ..Default::default()
    })
}

/// The mux domain for `host`, registering it if this is its first use.
pub fn domain_for_host(config: &ConfigHandle, host: &str) -> anyhow::Result<Arc<dyn Domain>> {
    let dom = domain_config_for_host(config, host)?;
    let mux = Mux::get();

    if let Some(existing) = mux.get_domain_by_name(&dom.name) {
        return Ok(existing);
    }

    let domain: Arc<dyn Domain> = Arc::new(ClientDomain::new(ClientDomainConfig::Ssh(dom)));
    mux.add_domain(&domain);
    Ok(domain)
}

#[cfg(test)]
mod test {
    use super::*;

    fn remote(name: &str, address: &str, path: Option<&str>) -> SshDomain {
        SshDomain {
            name: name.to_string(),
            remote_address: address.to_string(),
            multiplexing: SshMultiplexing::WezTerm,
            remote_wezterm_path: path.map(ToString::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn finds_a_configured_host_by_address_or_domain_name() {
        let configured = vec![remote("work", "build-box", Some("/opt/wezmux/bin/wezterm"))];

        for name in ["build-box", "work", "build-box:2222"] {
            let found = find_configured(configured.clone(), name)
                .unwrap_or_else(|| panic!("{name} should match"));
            assert_eq!(
                found.remote_wezterm_path.as_deref(),
                Some("/opt/wezmux/bin/wezterm")
            );
        }

        assert!(find_configured(configured, "other-box").is_none());
    }

    #[test]
    fn unconfigured_host_gets_an_ad_hoc_domain() {
        let config = config::configuration();
        let dom = domain_config_for_host(&config, "me@build-box:2222").unwrap();

        assert_eq!(dom.name, "SSHMUX:me@build-box:2222");
        assert_eq!(dom.remote_address, "build-box:2222");
        assert_eq!(dom.username.as_deref(), Some("me"));
        assert_eq!(dom.multiplexing, SshMultiplexing::WezTerm);
        // Resolved on connect rather than guessed here.
        assert_eq!(dom.remote_wezterm_path, None);
        // Without this, predictive echo is off and every keystroke waits for a
        // round trip.
        assert_eq!(
            dom.local_echo_threshold_ms,
            config::default_local_echo_threshold_ms()
        );
        assert!(dom.local_echo_threshold_ms.is_some());
    }

    #[test]
    fn history_is_recency_ordered_and_deduplicated() {
        let mut sessions = RemoteSessions::default();
        sessions.record("build-box", "api", "2026-01-01T00:00:00Z".to_string());
        sessions.record("other-box", "web", "2026-01-02T00:00:00Z".to_string());
        sessions.record("build-box", "api", "2026-01-03T00:00:00Z".to_string());

        assert_eq!(sessions.sessions.len(), 2);
        assert_eq!(sessions.sessions[0].host, "build-box");
        assert_eq!(sessions.sessions[0].last_used, "2026-01-03T00:00:00Z");
        assert_eq!(sessions.hosts(), vec!["build-box", "other-box"]);
    }

    #[test]
    fn history_is_capped() {
        let mut sessions = RemoteSessions::default();
        for i in 0..MAX_REMEMBERED + 5 {
            sessions.record("build-box", &format!("ws{i}"), "2026-01-01T00:00:00Z".to_string());
        }
        assert_eq!(sessions.sessions.len(), MAX_REMEMBERED);
        // The most recent survives, the oldest is dropped.
        assert_eq!(
            sessions.sessions[0].workspace,
            format!("ws{}", MAX_REMEMBERED + 4)
        );
    }

    #[test]
    fn picker_offers_local_first_and_a_way_out_last() {
        let entries = build_picker_entries(&RemoteSessions::default(), &PickerContext::default());

        assert_eq!(entries.first().unwrap().target, WorkspaceTarget::Local);
        assert_eq!(
            entries.last().unwrap().target,
            WorkspaceTarget::PromptForHost
        );
        // Nothing known: just the two escape hatches, so the caller can skip
        // the picker entirely.
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn picker_lists_remembered_sessions_then_hosts() {
        let mut sessions = RemoteSessions::default();
        sessions.record("build-box", "api", "2026-01-01T00:00:00Z".to_string());

        let ctx = PickerContext {
            configured_hosts: vec!["other-box".to_string()],
            ..Default::default()
        };
        let entries = build_picker_entries(&sessions, &ctx);
        let targets: Vec<_> = entries.iter().map(|e| e.target.clone()).collect();

        assert_eq!(
            targets,
            vec![
                WorkspaceTarget::Local,
                WorkspaceTarget::Remote {
                    host: "build-box".to_string(),
                    workspace: "api".to_string()
                },
                WorkspaceTarget::NewRemote {
                    host: "build-box".to_string()
                },
                WorkspaceTarget::NewRemote {
                    host: "other-box".to_string()
                },
                WorkspaceTarget::PromptForHost,
            ]
        );
    }

    #[test]
    fn a_host_is_offered_once_even_when_configured_and_remembered() {
        let mut sessions = RemoteSessions::default();
        sessions.record("build-box", "api", "2026-01-01T00:00:00Z".to_string());

        let ctx = PickerContext {
            configured_hosts: vec!["build-box".to_string()],
            ..Default::default()
        };
        let new_rows = build_picker_entries(&sessions, &ctx)
            .into_iter()
            .filter(|e| matches!(e.target, WorkspaceTarget::NewRemote { .. }))
            .count();

        assert_eq!(new_rows, 1);
    }

    #[test]
    fn a_running_session_says_so_instead_of_its_age() {
        let mut sessions = RemoteSessions::default();
        sessions.record("build-box", "api", "2026-01-01T00:00:00Z".to_string());
        sessions.record("build-box", "web", "2026-01-01T00:00:00Z".to_string());

        let ctx = PickerContext {
            live_workspaces: vec!["web".to_string()],
            attached_hosts: vec!["build-box".to_string()],
            ..Default::default()
        };
        let entries = build_picker_entries(&sessions, &ctx);

        let web = entries.iter().find(|e| e.label.starts_with("web")).unwrap();
        let api = entries.iter().find(|e| e.label.starts_with("api")).unwrap();
        assert_eq!(web.detail, "running");
        assert_ne!(api.detail, "running");
    }

    #[test]
    fn ages_read_as_relative_times() {
        let now = chrono::Utc::now();
        assert_eq!(
            describe_last_used(&(now - chrono::Duration::hours(3)).to_rfc3339()),
            "3h ago"
        );
        assert_eq!(
            describe_last_used(&(now - chrono::Duration::days(2)).to_rfc3339()),
            "2d ago"
        );
        assert_eq!(describe_last_used("not a timestamp"), "earlier");
    }

    #[test]
    fn forgetting_a_host_drops_all_its_sessions() {
        let mut sessions = RemoteSessions::default();
        sessions.record("build-box", "api", "2026-01-01T00:00:00Z".to_string());
        sessions.record("other-box", "web", "2026-01-01T00:00:00Z".to_string());

        sessions.forget_host("build-box");
        assert_eq!(sessions.hosts(), vec!["other-box"]);
    }
}
