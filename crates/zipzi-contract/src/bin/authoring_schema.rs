//! Export the shared v4 authoring contract without SDK/Runtime dependencies.
use zipzi_contract::api_shape::ApiShape;
fn main() {
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "protocol":zipzi_contract::protocol::CONTRACT,
            "command":zipzi_contract::kernel_commands::Command::api_shape(),
            "operation":zipzi_contract::kernel_wire::Operation::api_shape(),
            "source":zipzi_contract::kernel::QuerySource::api_shape(),
            "query":zipzi_contract::kernel_wire::Query::api_shape(),
            "asset":zipzi_contract::kernel::AssetRecord::api_shape(),
        }))
        .unwrap()
    );
}
