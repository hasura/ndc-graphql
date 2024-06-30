# GraphQL Native Data Connector for Hasura DDN

This GraphQL connector is intended to provide support for integrating Hasura V2
projects into Hasura V3 projects as subgraphs. In addition to this immediate
feature, the connector can connect many other GraphQL schemas similar to the
"Remote Schema" feature in Hasura V2.

This is implemented by providing a connector that translates the root-fields
of a GraphQL schema to NDC commands (function/procedures). Recent support for
field arguments and header forwarding allow the connector to represent the
majority of V2 queries/mutations.

**Current Limitations Include**

* Support for interfaces and unions is not currently available so schemas using these features are not fully supported.
* The V2 and V3 projects must share an auth provider in order to support JWT query authorization

## Usage

* Referencing from the Hub
* Plugin
* Configuration
* OpenDD Configuration

## Execution

* Architecture diagram
* Command pattern
* Field Arguments
* Header forwarding - forward / reverse

## Schemas

* Selection of schema in plugin
* Issues with "only-one-schema" and work arounds

## Authorization Use-Cases

* Admin secret mode - Dangerous needs V3 Permissions
* Shared JWT provider mode - Timeout scenario
* Independent auth scenario - Not supported

## Limitations

* Special header mapping - multiple set-cookie's etc.
* Pattern matching
* Pulling items out of session?

## Roadmap

Future Auth scenario support

## Development

Running the connector with Docker compose loop.