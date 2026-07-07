use crate::proto;
use relay_datastore::Storage;
use relay_usecase::RegisterValidatorUseCase;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{error, info};

pub struct ValidatorServiceImpl<S: Storage> {
    usecase: Arc<RegisterValidatorUseCase<S>>,
}

impl<S: Storage> ValidatorServiceImpl<S> {
    pub fn new(usecase: RegisterValidatorUseCase<S>) -> Self {
        Self {
            usecase: Arc::new(usecase),
        }
    }
}

#[tonic::async_trait]
impl<S> proto::validator_service_server::ValidatorService for ValidatorServiceImpl<S>
where
    S: Storage + Send + Sync + 'static,
{
    async fn register_validator(
        &self,
        request: Request<proto::RegisterValidatorRequest>,
    ) -> Result<Response<proto::RegisterValidatorResponse>, Status> {
        let req = request.into_inner();

        let proto_reg = match req.registration {
            Some(reg) => reg,
            None => {
                return Err(Status::invalid_argument("missing registration"));
            }
        };

        let registration = match proto_reg.try_into() {
            Ok(reg) => reg,
            Err(e) => {
                error!("conversion error: {e}");
                return Err(Status::invalid_argument(format!(
                    "invalid registration: {e}"
                )));
            }
        };

        match self.usecase.execute(registration).await {
            Ok(()) => {
                info!("validator registered successfully");
                Ok(Response::new(proto::RegisterValidatorResponse {
                    code: 0,
                    message: "ok".into(),
                }))
            }
            Err(e) => {
                error!("registration failed: {e}");
                Err(Status::internal(e.to_string()))
            }
        }
    }
}
