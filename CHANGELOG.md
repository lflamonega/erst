# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `erst search <words>` queries Docker Hub and lists matching images with
  their stars, pull counts and description; Docker's own images are marked `*`
  and catalog matches are pointed out. Works without a registry login.
- `/` on the dashboard opens the same search: type, `enter` to search, arrows
  to pick, `enter` to install. Images that are in the catalog install with
  their usual ports and settings.
- Resource limits: `erst install --memory 512m --cpu 1.5` caps an app from the
  start, and `erst limit <app> --memory … --cpu …` changes or lifts those caps
  later. A bare `--memory 512` means megabytes rather than the 512 bytes the
  container convention would read, and `unlimited` clears a cap. `erst list`
  and the dashboard show them in a new `LIMITS` column.

### Changed

- `erst update` now reports when an app declares no volumes, so it is visible
  that data written inside the container does not survive the rebuild.

### Fixed

- The dashboard's `r` (catalog) and other top-level keys did nothing while no
  app was installed, which is exactly when the empty state tells you to press
  them.

## [0.2.0] - 2026-10-02

### Added

- `erst dashboard` (also what plain `erst` runs): a terminal dashboard with the
  installed apps, their status, ports and user, a log viewer, and start, stop,
  update, backup, install and remove actions. The list refreshes on its own.
- Commands now report what they did instead of printing it, so the dashboard can
  show it in its status line without a command writing over the screen.
- `erst backup <app> [file]` writes a `tar.gz` with the app settings and a tar
  per data volume; the file name defaults to `<app>-<timestamp>.tar.gz`.
- `erst restore <file>` recreates an app from a backup and repopulates its
  volumes.
- Named volumes for the paths an image declares as volumes (`erst-data-<app>-<n>`),
  so data survives updates and can be backed up and removed.
- Hardening by default: `no-new-privileges` plus dropping `AUDIT_WRITE`, `MKNOD`
  and `SETFCAP`, with `erst install --no-harden` as the escape hatch.
- A warning when an installed image runs as root, and a `root!` marker in
  `erst list` so it stays visible.
- A post-install check that reports the last log lines when an app exits or
  crash-loops instead of reporting a successful install.
- `erst list --check-updates` asks each registry whether a newer image exists,
  comparing the registry manifest digest with the local one. Registries are
  queried in parallel with anonymous tokens, and an unreachable or private
  registry shows `?` instead of blocking the listing.

### Fixed

- Windows builds: the runtime probe uses the Docker Desktop named pipe on
  Windows, since bollard's Podman and Unix socket constructors do not exist
  there.

## [0.1.0] - 2026-10-02

### Added

- Flexible container runtime detection: Docker (rootful or rootless) and Podman
  rootless, probed automatically without requiring root.
- `erst install` for catalog apps and arbitrary container images, with
  `--port`, `--env` and `--host` flags.
- Built-in catalog (`erst catalog`) with nginx, redis, postgres, mariadb, minio,
  vaultwarden and uptime-kuma.
- `erst list` showing installed apps, running ones first, with published ports.
- `erst logs` with `--follow` and `--tail`.
- `erst start`, `erst stop`, `erst restart` for app lifecycle.
- `erst update` pulling the latest image and recreating the container, keeping
  its data.
- `erst remove` keeping data by default, `--remove-data` to delete it.
- Apps restart automatically (`unless-stopped`) and their settings are stored as
  a label on the container, so no external state file is needed.

[Unreleased]: https://github.com/lflamonega/erst/compare/v0.2.0...develop
[0.2.0]: https://github.com/lflamonega/erst/releases/tag/v0.2.0
[0.1.0]: https://github.com/lflamonega/erst/releases/tag/v0.1.0
