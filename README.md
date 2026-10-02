# erst

Install and manage containers from the command line, without knowing containers.

`erst` is a friendly CLI on top of a container runtime (Docker or Podman). It
lets you install well-known apps from a built-in catalog — or any container
image — and manage them with short, human commands.

## Requirements

- A running container runtime: Docker (rootful or rootless) or Podman (rootless).
  `erst` probes the available endpoints automatically (Unix sockets on Linux and
  macOS, the Docker Desktop named pipe on Windows); no root is needed as long as
  your user can access the runtime.
- Linux, macOS or Windows.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/lflamonega/erst/develop/install.sh | sh
```

Or build from source:

```sh
cargo install --path .
```

## Usage

```sh
erst catalog                 # list the apps in the catalog
erst install nginx           # install a catalog app
erst install postgres        # databases included
erst install myorg/myapp --port 8080:80   # any image
erst list                    # what is installed, running first
erst list --check-updates    # also ask each registry for a newer image
erst logs nginx -f           # follow the logs
erst stop nginx              # stop / start / restart an app
erst update nginx            # pull and roll out the latest image
erst remove nginx            # keeps the data
erst remove nginx --remove-data
erst backup postgres         # save the data to <app>-<timestamp>.tar.gz
erst restore postgres-1730.tar.gz   # bring the app back from a backup
```

`install` accepts:

- `--port HOST:CONTAINER` (repeatable) to publish ports; catalog apps publish
  sensible defaults.
- `--env KEY=VALUE` (repeatable) for configuration.
- `--host` to record the hostname the app is served under.
- `--no-harden` to skip hardening for apps that need to escalate privileges.

## Data and backups

When an app declares volumes, `erst` puts each of them in a named volume
(`erst-data-<app>-<n>`), which is what makes data survive updates and lets
`erst backup` and `erst remove --remove-data` work.

`erst backup <app>` writes a `tar.gz` holding the app settings plus a tar per
volume, and `erst restore <file>` recreates the app and repopulates those
volumes. Backups are plain files: copy them wherever you like, and test a
restore from time to time — an untested backup is a guess.

Apps that declare no volumes (`nginx`, `alpine`, …) keep their state in the
container filesystem and cannot be backed up; `erst backup` says so instead of
writing an empty archive.

## Containers and root

Most public images (nginx, postgres, redis, …) start as root **inside the
container**. That is not root on the machine — the container keeps its own
namespaces, reduced capabilities and seccomp filters — but a container from an
untrusted image is still more dangerous than it looks if it ever escapes.

So `erst`:

- **warns on install** when an image runs as root, so you see it instead of
  finding out later;
- **shows the user in `erst list`** (`root!` marks a container running as root);
- **hardens containers it creates** by applying `no-new-privileges` and dropping
  the `AUDIT_WRITE`, `MKNOD` and `SETFCAP` capabilities, which ordinary apps do
  not need and which widen the damage a container escape could do;
- **checks that the app actually stayed up** after starting, printing the last
  log lines if it exited or crash-loops.

`erst` never changes the user of an image: doing so breaks most popular apps,
which bind privileged ports or drop privileges themselves. If an app fails to
start because of the hardening, install it with `erst install <app> --no-harden`
and it runs with the default capabilities.

## Development

```sh
make check    # fmt --check + clippy + tests
make test     # unit tests
make build    # release build
```

Conventions: Conventional Commits, development on `develop` through pull
requests, releases tagged `vX.Y.Z` and described in [CHANGELOG.md](CHANGELOG.md).

## License

MIT
