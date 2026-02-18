mod handler;

use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};

use crate::state::AppState;
use handler::LatticeMcpServer;

pub fn service(state: AppState) -> StreamableHttpService<LatticeMcpServer> {
    let db = state.db.clone();
    StreamableHttpService::new(
        move || Ok(LatticeMcpServer::new(db.clone())),
        Default::default(),
        StreamableHttpServerConfig::default(),
    )
}
