use serde::Serialize;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("记录不存在：{0}")]
    Missing(String),
    #[error("资料已有更新，请重新加载后再保存")]
    Conflict,
    #[error("正在恢复资料库，请稍后再试")]
    Restoring,
    #[error("前面的保存失败，已中止恢复，请先重试保存")]
    Unsaved,
    #[error("数据库版本较新，当前应用无法打开")]
    Newer,
    #[error("恢复未完成，已保留恢复记录与旧库：{0}")]
    Recovery(String),
    #[error("数据库工作线程不可用")]
    Unavailable,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}
impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let code = match error {
            Error::Conflict => "conflict",
            Error::Restoring => "restoring",
            Error::Unsaved => "unsaved",
            Error::Newer => "newerVersion",
            Error::Missing(_) => "notFound",
            Error::Invalid(_) => "invalid",
            Error::Recovery(_) => "recoveryRequired",
            _ => "storage",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}
