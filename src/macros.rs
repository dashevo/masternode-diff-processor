#[macro_export]
macro_rules! unwrap_or_return {
    ($e: expr, $re: expr) => {
        match $e {
            Some(x) => x,
            None => { return $re() },
        }
    }
}

#[macro_export]
macro_rules! unwrap_or_failure {
    ($e: expr) => { unwrap_or_return!($e, || boxed(types::MNListDiffResult::default())) }
}

#[macro_export]
macro_rules! unwrap_or_qr_failure {
    ($e: expr) => { unwrap_or_return!($e, || boxed(types::LLMQRotationInfo::default())) }
}

#[macro_export]
macro_rules! unwrap_or_qr_result_failure {
    ($e: expr) => { unwrap_or_return!($e, || boxed(types::LLMQRotationInfoResult::default())) }
}
