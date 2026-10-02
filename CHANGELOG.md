# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/lflamonega/erst/compare/v0.1.0...develop
[0.1.0]: https://github.com/lflamonega/erst/releases/tag/v0.1.0
