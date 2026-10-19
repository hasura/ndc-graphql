# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0]

- Fix batched function responses: return one RowSet per variable set in input order, using `__value` rather than upstream `qN__value` aliases.
- Return no RowSets and make no upstream request for an explicitly empty variables batch.
- Reject missing function-result aliases instead of silently converting malformed responses to null.
- Accept empty GraphQL error arrays without panicking; preserve failures for nonempty errors.
- Coerce built-in scalar arguments, including nested input objects and lists, to upstream types. String identifiers retain their existing leading zeros.
- Add HTTP-backed production-handler regression tests for direct and batched list-valued commands, string identifiers, headers, null/empty results, and upstream failures.
- Pin build tooling, use the committed lockfile in release builds, and gate publication on tests, formatting, and security audit.
- Publish the CLI manifest with the other release assets.
- Update locked dependencies for RustSec findings and replace the unmaintained `rustls-pemfile` wrapper with the maintained `rustls-pki-types` PEM API. Security checks reject advisory warnings as well as vulnerabilities.

The changes below were previously listed under an unpublished 0.3.1 heading.

- Add support for custom CA certificates and allow skipping TLS verification entirely
- Breaking: GraphQL `interface` and `union` output types are now lowered into tagged-variant NDC object types by default.
  - Lowered object types include `__typename` and one nullable field per concrete type (`on_<ConcreteType>`).
  - Query generation translates tagged variant selections to GraphQL inline fragments (`... on <ConcreteType>`).
  - Validation now fails early with clear errors for unsupported polymorphic schemas (for example unions/interfaces without concrete object members, or unresolved type references).

## [0.3.0]

- Upgrade ndc-spec v0.2.10 and ndc-rust-sdk
- Support request-level arguments

## [0.2.3]

- Patch release for v0.2.1 and v0.2.2

## [0.2.2]

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
