use ndc_sdk::models;

pub fn capabilities() -> models::CapabilitiesResponse {
    models::CapabilitiesResponse {
        version: "0.1.4".to_string(),
        capabilities: models::Capabilities {
            query: models::QueryCapabilities {
                aggregates: None,
                variables: Some(models::LeafCapability {}),
                explain: Some(models::LeafCapability {}),
                nested_fields: models::NestedFieldCapabilities {
                    aggregates: None,
                    filter_by: None,
                    order_by: None,
                },
            },
            mutation: models::MutationCapabilities {
                transactional: None,
                explain: Some(models::LeafCapability {}),
            },
            relationships: None,
        },
    }
}
