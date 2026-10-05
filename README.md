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
erst dashboard               # interactive TUI (also the default with no arguments)
erst catalog                 # list the apps in the catalog
erst search redis            # search Docker Hub for any image
erst install nginx           # install a catalog app
erst install postgres        # databases included
erst install myorg/myapp --port 8080:80   # any image
erst install nginx --memory 256m --cpu 0.5  # cap what it may use
erst install mysql --network blog   # let apps on `blog` reach it as `mysql`
erst limit nginx --memory 512m             # change (or lift) those caps
erst enable nginx                          # start it when you log in
erst disable nginx
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
- `--memory` and `--cpu` to cap resources (see below).
- `--network NAME` to put the app on a shared network (see below).
- `--no-harden` to skip hardening for apps that need to escalate privileges.

## Finding an image

`erst search <words>` looks on Docker Hub, and so does `/` in the dashboard.
Rows marked `*` are published by Docker itself, which is the safe choice when
you cannot judge an image by its author. When a match is also in the built-in
catalog, `erst` says so: catalog apps install with sensible ports and settings,
while an arbitrary image installs bare.

An image installed bare publishes no ports, so nothing is reachable from your
machine. That is normal for a service another app talks to; for anything you
want to open yourself, install it with `--port HOST:CONTAINER`.

## Apps that talk to each other

Apps installed separately can only meet through ports published on your
machine. Give them a shared network instead and they find each other by name:

```sh
erst install mysql --network blog
erst install wordpress --network blog
```

Both land on `erst-net-blog`, where `wordpress` reaches the database at
`mysql:3306`. The name it answers to is the app's own name, not the container
name `erst-app-mysql`. The network is built the first time an app asks for it
and `erst remove` takes it away again once the last app on it has gone.

Being on a shared network also means being *off* the runtime's default one: an
app on `blog` reaches nothing that is not on `blog` with it. That is usually
the point — it is what turns a group of apps into a group rather than a list —
and it costs nothing in the other direction, since outbound access to the
internet works exactly as it did before.

`erst list` shows the network in the `NET` column; the dashboard puts it in
brackets on the app's row.

## Resource limits

Nothing is capped by default — an uncapped app can use every core and all the
memory your machine has, which is usually fine and occasionally disastrous. Cap
the risky ones:

```sh
erst install postgres --memory 1g --cpu 2
erst limit nginx --memory 256m          # change a running app
erst limit nginx --memory unlimited     # take the cap away again
```

`--memory` takes `512m`, `1g` or a bare `512`, and a bare number means
**megabytes**: `--memory 512` is 512 MB, not 512 bytes. `--cpu` takes a number
of CPUs, so `0.5` is half a core. `erst list` and the dashboard show both in a
`LIMITS` column (`-` when nothing is capped).

Limits are part of a container's settings, and a container's settings can only
be set when it is created, so `erst limit` rebuilds the app. Data in the
declared volumes survives that; for an app with no volumes `erst` says up front
that anything written inside the container will be replaced.

## Starting at login

A container restarts on its own whenever its runtime comes back — that is what
`restart: unless-stopped` means — but nothing restarts it after the whole
machine reboots when the runtime starts with your session or is daemonless, as
Podman is. `erst enable <app>` closes that gap:

```sh
erst enable nginx        # start nginx when you log in
erst disable nginx       # stop doing that
```

It writes a systemd **user** unit on Linux or a launch agent on macOS, in the
directory the operating system already reads, so no root is needed at any
point. `erst list` shows which apps are set up this way in the `BOOT` column
(`auto`), and the dashboard toggles the selected app with `e`.

Two things to keep apart: `erst stop <app>` stops it now, while `erst disable
<app>` decides what happens at login. An app that is stopped but still enabled
starts again the next time you log in, so disable it if you want it to stay
off. `erst remove` drops the entry along with the app.

On a machine without systemd running (containers, WSL) or on Windows there is
nowhere to put such a service, and `erst enable` says so rather than pretending.

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

## Dashboard

`erst dashboard` (or `erst` with no arguments) opens a terminal UI:

| key   | action                                             |
|-------|----------------------------------------------------|
| `j/k` | move between apps                                   |
| `l`   | logs of the selected app                            |
| `s`   | start                                               |
| `x`   | stop                                                |
| `u`   | update to the latest image                          |
| `b`   | back up the data                                    |
| `d`   | back up the data, then remove the app               |
| `e`   | start at login (`erst enable` / `erst disable`)     |
| `r`   | install from the catalog                            |
| `/`   | search Docker Hub for any image                     |
| `R`   | show only apps running as root                      |
| `q`   | quit                                               |

The list refreshes on its own, and the header shows how many apps run as root.

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
