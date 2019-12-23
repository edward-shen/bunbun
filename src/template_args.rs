use serde::Serialize;

pub fn query(query: String) -> impl Serialize {
  #[derive(Serialize)]
  struct TemplateArgs {
    query: String,
  }
  TemplateArgs { query }
}

pub fn hostname(hostname: String) -> impl Serialize {
  #[derive(Serialize)]
  pub struct TemplateArgs {
    pub hostname: String,
  }
  TemplateArgs { hostname }
}
