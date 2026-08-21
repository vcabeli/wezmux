//! Manual checks for the workspace metadata request that backs the sidebar of a
//! remote session, against a real mux server over the real protocol.
//!
//! Unit tests cannot cover this part: the server is the side that resolves a
//! workspace's panes and runs git/gh/lsof, and the answer comes back over the
//! wire. Both tests are ignored by default because each needs a running server.
//!
//! Against a local mux server (same code path as a remote one, minus the ssh
//! transport):
//!
//! ```sh
//! target/debug/wezterm cli --prefer-mux spawn --new-window \
//!     --workspace selftest --cwd /path/to/a/git/repo
//! WS=selftest cargo test -p wezterm-client --test live_remote_metadata \
//!     -- --ignored --nocapture local_mux_reports_workspace_metadata
//! ```
//!
//! Against a host with wezmux installed (see `bin/wezmux-install-remote`),
//! which additionally covers the ssh transport and the version handshake that
//! rejects a non-wezmux server:
//!
//! ```sh
//! ssh build-box '/path/to/remote/wezterm cli --prefer-mux spawn --new-window \
//!     --workspace selftest --cwd /path/to/a/git/repo/on/that/host'
//! WEZMUX_TEST_SSH_HOST=build-box \
//! WEZMUX_TEST_REMOTE_PATH=/path/to/remote/wezterm \
//! WS=selftest cargo test -p wezterm-client --test live_remote_metadata \
//!     -- --ignored --nocapture remote_mux_over_ssh_reports_workspace_metadata
//! ```
//!
//! Expect the snapshot to report that repo's branch and dirty state.

use codec::GetWorkspaceMetadata;
use config::{SshDomain, SshMultiplexing, UnixDomain};
use mux::connui::ConnectionUI;
use mux::workspace_metadata::WorkspaceMetadataSnapshot;
use wezterm_client::client::Client;

fn workspace() -> String {
    std::env::var("WS").unwrap_or_else(|_| "default".to_string())
}

/// Ask `client` for `workspace`'s metadata and report what came back.
fn fetch(client: &Client, workspace: &str) -> Option<WorkspaceMetadataSnapshot> {
    let response = promise::spawn::block_on(client.get_workspace_metadata(GetWorkspaceMetadata {
        workspace: workspace.to_string(),
        want_pull_request: false,
    }))
    .expect("get_workspace_metadata");

    assert_eq!(response.workspace, workspace);
    println!("{workspace}: {:#?}", response.metadata);
    response.metadata
}

/// The server resolves the cwd from its own panes, so this only holds when the
/// workspace exists there with a pane sitting in a git repository.
fn assert_reports_a_repo(metadata: Option<WorkspaceMetadataSnapshot>) {
    let metadata = metadata.expect("the server should have resolved a cwd for this workspace");
    if std::env::var("WS").is_ok() {
        assert!(
            metadata.is_git_repo,
            "expected the repo's git state, got {:?}",
            metadata
        );
        assert!(metadata.git_branch.is_some());
    }
}

#[test]
#[ignore = "needs a running mux server; see the module comment"]
fn local_mux_reports_workspace_metadata() {
    config::common_init(None, &[], true).unwrap();
    let _executor = promise::spawn::SimpleExecutor::new();

    let mut ui = ConnectionUI::new_headless();
    let dom = UnixDomain::default_unix_domains().remove(0);
    let client =
        Client::new_unix_domain(None, &dom, true, &mut ui, false).expect("connect to mux server");

    assert_reports_a_repo(fetch(&client, &workspace()));
}

#[test]
#[ignore = "needs a remote host with wezmux installed; see the module comment"]
fn remote_mux_over_ssh_reports_workspace_metadata() {
    let Ok(host) = std::env::var("WEZMUX_TEST_SSH_HOST") else {
        panic!("set WEZMUX_TEST_SSH_HOST to the remote host to test against");
    };
    config::common_init(None, &[], true).unwrap();
    // The client queues background work on the main-thread scheduler; without
    // one configured, its housekeeping tasks panic as the test tears down.
    let _executor = promise::spawn::SimpleExecutor::new();

    let dom = SshDomain {
        name: format!("SSHMUX:{host}"),
        remote_address: host.clone(),
        multiplexing: SshMultiplexing::WezTerm,
        remote_wezterm_path: std::env::var("WEZMUX_TEST_REMOTE_PATH").ok(),
        ..Default::default()
    };

    let mut ui = ConnectionUI::new_headless();
    let client = Client::new_ssh(mux::domain::alloc_domain_id(), &dom, &mut ui)
        .expect("connect to the remote mux server over ssh");

    // A stock or mismatched server reports a different codec version.
    let info =
        promise::spawn::block_on(client.verify_version_compat(&ui)).expect("version handshake");
    println!(
        "server version {:?}, codec {}",
        info.version_string, info.codec_vers
    );

    assert_reports_a_repo(fetch(&client, &workspace()));
}
