# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/luxass/actioneer/compare/v0.2.0...v0.3.0) - 2026-07-13

### 🐛 Bug Fixes

- *(scan)* fall back to eligible aged releases ([#30](https://github.com/luxass/actioneer/pull/30)) (by @luxass)
- *(audit)* report verifiable SHA provenance ([#33](https://github.com/luxass/actioneer/pull/33)) (by @luxass)
- *(scan)* skip remote checks for local workflows ([#32](https://github.com/luxass/actioneer/pull/32)) (by @luxass)
- *(ci)* repair release test command ([#29](https://github.com/luxass/actioneer/pull/29)) (by @luxass)
- *(cli)* honor apply execution modes ([#31](https://github.com/luxass/actioneer/pull/31)) (by @luxass)

### 📚 Documentation

- simplify rewrite rules model (by @luxass)
- add rewrite specification (by @luxass)

### 🚀 Features

- complete workflow pin auditing architecture ([#28](https://github.com/luxass/actioneer/pull/28)) (by @luxass)

### Contributors

* @luxass

## [0.2.0](https://github.com/luxass/actioneer/compare/v0.1.17...v0.2.0) - 2026-06-13

### 🚀 Features

- add minimum release age guard ([#26](https://github.com/luxass/actioneer/pull/26)) (by @luxass)
- add --filter flag to target specific actions non-interactively ([#25](https://github.com/luxass/actioneer/pull/25)) (by @luxass)

### 🚜 Refactor

- extract shared command pipeline, expand tests, enforce e2e in CI ([#20](https://github.com/luxass/actioneer/pull/20)) (by @luxass)

### Contributors

* @luxass

## [0.1.17](https://github.com/luxass/actioneer/compare/v0.1.16...v0.1.17) - 2026-06-11

### 🐛 Bug Fixes

- tidy update output ([#19](https://github.com/luxass/actioneer/pull/19)) (by @luxass)

### 💼 Other

- Refactor module layout and test harnesses ([#14](https://github.com/luxass/actioneer/pull/14)) (by @luxass)
- flat module layout, single Action type ([#13](https://github.com/luxass/actioneer/pull/13)) (by @luxass)

### 🚜 Refactor

- restructure ActionReference and WorkflowEdit ([#16](https://github.com/luxass/actioneer/pull/16)) (by @luxass)
- split action references from resolved updates ([#15](https://github.com/luxass/actioneer/pull/15)) (by @luxass)

### Contributors

* @luxass
