# Releasing ndc-graphql

This repository builds release candidates on PRs/main, but publishes only on
`v*` tags. Do not tag until the release-preparation PR has been reviewed.

1. Update the workspace version, local `Cargo.lock` package versions, and matching
   changelog section. Run `python3 ci/check-release-version.py`.
2. Pass the test/format, security audit, container build, and all five CLI build
   targets on the release candidate. Tests/audit/version checks also gate the tag
   workflow before image publication. Do not suppress unresolved advisories.
3. Validate direct and cross-subgraph batched command execution with representative
   metadata on the customer's exact engine. The HTTP-backed handler tests are
   connector regression evidence, **not** engine compatibility certification.
   Include a model-to-command relationship returning a list of objects, with
   representative string identifiers and the deployed engine version.
4. After approval, tag the reviewed commit `v<workspace version>`. The workflow
   publishes `ghcr.io/hasura/ndc-graphql:v<version>`, five CLI binaries,
   `manifest.yaml`, and `connector-definition.tgz`. Tag publication is not draft.
5. Check the image's source revision/digest, architecture, CLI versions/checksums,
   manifest download URLs, and connector-definition image/plugin versions.
   The CLI matrix does not imply a multiarchitecture container.
6. Register the CLI/plugin and connector catalog via the approved distribution
   process. GHCR publication does not automatically deliver an image to a
   self-hosted/EE registry or air-gapped deployment. Confirm the customer's route
   and rollback configuration before rollout.

## 0.4.0 migration

Interface/union outputs now expose `__typename` and nullable `on_<ConcreteType>`
fields. Regenerate connector schema/metadata in a development copy, inspect the
diff, and update affected selections before rollout. Retain an old-image/metadata
rollback pair. Existing string CIFs retain leading zeros; coercion cannot recover
zeros already lost by converting an identifier to a number.

For organization-signed upstream TLS certificates, mount PEM certificates and
set `GRAPHQL_CA_CERT_FILE` and/or `GRAPHQL_CA_CERT_DIR` in the connector process.
Do not use `GRAPHQL_INSECURE_SKIP_TLS_VERIFY` in production.
