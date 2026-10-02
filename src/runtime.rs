use anyhow::{Result, bail};
use bollard::Docker;

/// Connect to the first container runtime available on this host.
///
/// The probe order is intentionally broad: Podman rootless sockets first (they
/// work without any special group membership), then the regular Docker socket.
/// Each candidate is pinged before being accepted, so a stale socket file does
/// not break the CLI.
pub async fn connect() -> Result<Docker> {
    for candidate in candidates() {
        let docker = match candidate {
            Ok(docker) => docker,
            Err(_) => continue,
        };
        if docker.ping().await.is_ok() {
            return Ok(docker);
        }
    }

    bail!(
        "no container runtime found: tried Podman (rootless) and Docker. \
         Is the daemon running, and can your user access its socket?"
    )
}

fn candidates() -> [Result<Docker, bollard::errors::Error>; 2] {
    [
        Docker::connect_with_podman_defaults(),
        Docker::connect_with_local_defaults(),
    ]
}
