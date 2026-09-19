use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RpcObject(pub Value);

pub type RequestId = u64;

impl RpcObject {
    pub fn get_id(&self) -> Option<RequestId> {
        self.0.get("id").and_then(Value::as_u64)
    }

    pub fn is_response(&self) -> bool {
        self.0.get("id").is_some() && self.0.get("method").is_none()
    }

    pub fn into_response(mut self) -> Result<Result<Value, Value>, String> {
        let _ = self
            .get_id()
            .ok_or_else(|| "Response requires 'id' field.".to_string())?;

        if self.0.get("result").is_some() == self.0.get("error").is_some() {
            return Err("RPC response must contain exactly one of\
                        'error' or 'result' fields."
                .into());
        }
        let result = self.0.as_object_mut().and_then(|obj| obj.remove("result"));

        match result {
            Some(r) => Ok(Ok(r)),
            None => {
                let error = self
                    .0
                    .as_object_mut()
                    .and_then(|obj| obj.remove("error"))
                    .unwrap();
                Ok(Err(error))
            }
        }
    }
}
