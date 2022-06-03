use std::borrow::Cow;

use percent_encoding::PercentEncode;
use serde::Serialize;

pub fn query<'a>(query: PercentEncode<'a>) -> impl Serialize + 'a {
  #[derive(Serialize)]
  struct TemplateArgs<'a> {
    query: Cow<'a, str>,
  }
  TemplateArgs {
    query: query.into(),
  }
}

pub fn hostname<'a>(hostname: &'a str) -> impl Serialize + 'a {
  #[derive(Serialize)]
  pub struct TemplateArgs<'a> {
    pub hostname: &'a str,
  }
  TemplateArgs { hostname }
}
