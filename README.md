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

The connector has been designed to work best in its own subgraph. While it is possible to
use in an existing subgraph, we recommend [creating a subgraph](https://hasura.io/docs/3.0/getting-started/init-subgraph)
for the purposes of connecting to a GraphQL schema with this connector.
Once you are operating within a subgraph you can add the GraphQL connector:

```sh
ddn subgraph init app
cd app
ddn connector init graphql --hub-connector hasura/graphql
```


### Configuring the introspection role

Once the connector has been added it will expose its configuration in
`config/configuration.json`. You should update some values in this config before
performing introspection.

The configuration Is split into request/connection/introspection sections.
You should update the introspection of the configuration to have the
`x-hasura-admin-secret` and `x-hasura-role` headers set in order to allow
the introspection request to be executed.

```json
{
  ...
  "introspection": {
    "endpoint": {
      "value": "https://my-hasura-v2-service/v1/graphql"
    },
    "headers": {
      "X-Hasura-Admin-Secret": {
        "value": "my-super-secret-admin-secret"
      },
      "Content-Type": {
        "value": "application/json"
      }
    }
  }
}
```

Without an explicit role set this will use the admin role to fetch the schema, which
may or may not be appropriate for your application!

### Performing Introspection

Once the connector introspection configuration is updated, you can perform an update
in order to fetch the schema for use and then add the connector link:

```sh
# Benoit?
ddn connector introspect
ddn connector-link update graphql --add-all-resources
```

### Configuring the header passthrough behaviour

The connector link will probably need to be updated to pass through headers.

This is done with the following metadata configuration:

```yaml
kind: DataConnectorLink
version: v1
definition:
  name: graphql
  url:
    readWriteUrls:
      read:
        valueFromEnv: APP_GRAPHQL_READ_URL
      write:
        valueFromEnv: APP_GRAPHQL_WRITE_URL
  schema:
    # This is read from the connector schema configuration
  argumentPresets:
    - argument: headers
      value:
        httpHeaders:
          forward:
            - X-Hasura-Admin-Secret
            - Authorization
          additional: {}
```

You may also want to configuring the response header behaviour at this point if you
need to have response headers passed back to the client.

### Integrate into your supergraph

Track the associated commands (functions/procedures) in your supergraph:

```sh
# Benoit?
ddn-staging connector-link update graphql --add-all-resources
```

### Replicating specific permissions in models

While this may be sufficient if your schema and role matches,
if you wish to have additionally restrictive permissions imposed you may
do so at the model level with [the Hasura V3 permissions system](https://hasura.io/docs/3.0/supergraph-modeling/permissions).

### Removing namespacing

While this may be sufficient 

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

* Running the connector with Docker compose loop
* Provided resources
* `refresh.sh` script