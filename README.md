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

* Support for interfaces and unions is not currently available so schemas using
  these features are not fully supported
* The V2 and V3 projects must share an auth provider in order to support JWT query authorization
* Errors returned by the connector will be formatted differently

## Usage

The high-level steps for working with the GraphQL connector follows
the same pattern as any connector:

* Add the connector
* Configure the connector
* Integrate into your supergraph
* Configure in your supergraph

The main focus wrt. the GraphQL connector will be:

* Configuring the introspection role
* Configuring the header passthrough behaviour
* Configuring the argument preset and response header behaviour in the connector link
* Replicating specific permissions in models

All of the following steps assume you are working within an existing Hasura V3 project.
Please see [the V3 documentation](https://hasura.io/docs/3.0/getting-started/init-subgraph) for
information about getting started with Hasura V3.

Likewise, when using the connector to connect to a Hasura V2 project, you can see the
[V2 documentation](https://hasura.io/docs/latest/index/) for information about Hasura V2.

### Add the connector



### Configuring the introspection role
### Configuring the header passthrough behaviour
### Configuring the argument preset and response header behaviour in the connector link
### Integrate into your supergraph
### Configure in your supergraph
### Replicating specific permissions in models

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