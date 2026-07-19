# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This project uses [*towncrier*](https://towncrier.readthedocs.io/) and the
changes for the upcoming release are collected as fragments in
[changelog.d](changelog.d/). See `changelog.d/README.md` for the convention.

## [Unreleased]

### Known limitations

- Not yet published to crates.io or tagged as a release.
- `--scope user|project` is a Claude-Code-only concept; passing it with `--codex` is
  accepted but has no effect (Codex always writes the project-local `.codex-plugin/`
  bundle), and `tooned` warns on stderr when this happens rather than silently ignoring
  the flag.

<!-- towncrier release notes start -->
