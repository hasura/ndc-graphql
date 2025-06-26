# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1]

- Add nativeToolchainDefinition to connector-metadata.yml

## [0.2.0]

- Implement foreach capability. Instead of producing multiple parallel requests, we produce a single, larger request to send to the target endpoint.
- Fix bug where introspection including interfaces would fail to parse in some circumstances
- Config now defaults to asking for a `GRAPHQL_ENDPOINT` env var
- Fix a bug where default values were not parsed as graphql values, and instead used as string literals
- CLI: Implement `print-schema-and-capabilities` command, allowing local dev to update config & schema without starting a connector instance
- Update to latest connector SDK version (0.4.0)

## [0.1.3]

- Fix incorrect version being returned by capabilities

## [0.1.2]

- Fix issue where we looked for mutation fields in the query type, making all mutations fail

## [0.1.1]

- Forward errors from underlying source to users using 422 status code
- Change default configuration to not include header forwarding
- Handle error when unknown field argument is used

## [0.1.0]

- Initial Release
