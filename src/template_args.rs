use std::borrow::Cow;

use percent_encoding::PercentEncode;
use serde::Serialize;

pub fn query(query: PercentEncode<'_>) -> impl Serialize + '_ {
    #[derive(Serialize)]
    struct TemplateArgs<'a> {
        query: Cow<'a, str>,
    }
    TemplateArgs {
        query: query.into(),
    }
}

pub fn hostname(hostname: &'_ str) -> impl Serialize + '_ {
    #[derive(Serialize)]
    pub struct TemplateArgs<'a> {
        pub hostname: &'a str,
    }
    TemplateArgs { hostname }
}
