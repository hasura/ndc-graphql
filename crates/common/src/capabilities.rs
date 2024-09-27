use ndc_models as models;

pub fn capabilities() -> models::Capabilities {
    models::Capabilities {
        query: models::QueryCapabilities {
            aggregates: None,
            variables: Some(models::LeafCapability {}),
            explain: Some(models::LeafCapability {}),
            nested_fields: models::NestedFieldCapabilities {
                aggregates: None,
                filter_by: None,
                order_by: None,
            },
            exists: models::ExistsCapabilities {
                nested_collections: None,
            },
        },
        mutation: models::MutationCapabilities {
            transactional: None,
            explain: Some(models::LeafCapability {}),
        },
        relationships: None,
    }
}

pub fn capabilities_response() -> models::CapabilitiesResponse {
    models::CapabilitiesResponse {
        version: models::VERSION.into(),
        capabilities: capabilities(),
    }
}
