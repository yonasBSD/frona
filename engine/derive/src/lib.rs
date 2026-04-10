use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, ImplItem, ItemImpl, Lit, Meta, Token};
use syn::parse::{Parse, ParseStream};

mod migration;

#[proc_macro_attribute]
pub fn migration(attr: TokenStream, item: TokenStream) -> TokenStream {
    migration::migration(attr, item)
}

#[proc_macro_derive(Entity, attributes(entity))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let table = input
        .attrs
        .iter()
        .find_map(|attr| {
            if !attr.path().is_ident("entity") {
                return None;
            }
            let nested: Meta = attr.parse_args().ok()?;
            if let Meta::NameValue(nv) = nested
                && nv.path.is_ident("table")
                && let Expr::Lit(lit) = &nv.value
                && let Lit::Str(s) = &lit.lit
            {
                return Some(s.value());
            }
            None
        })
        .expect("#[entity(table = \"...\")] attribute is required");

    let expanded = quote! {
        impl frona::core::repository::Entity for #name {
            fn table() -> &'static str {
                #table
            }

            fn id(&self) -> &str {
                &self.id
            }
        }
    };

    expanded.into()
}

struct AgentToolArgs {
    name: Option<String>,
    files: Option<Vec<String>>,
}

impl Parse for AgentToolArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut files = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "name" => {
                    let _eq: Token![=] = input.parse()?;
                    let lit: syn::LitStr = input.parse()?;
                    name = Some(lit.value());
                }
                "files" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let mut file_list = Vec::new();
                    while !content.is_empty() {
                        let lit: syn::LitStr = content.parse()?;
                        file_list.push(lit.value());
                        if !content.is_empty() {
                            let _comma: Token![,] = content.parse()?;
                        }
                    }
                    files = Some(file_list);
                }
                other => {
                    return Err(syn::Error::new(ident.span(), format!("unknown agent_tool argument: {other}")));
                }
            }

            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(AgentToolArgs { name, files })
    }
}

fn derive_tool_name(struct_name: &str) -> String {
    let base = struct_name.strip_suffix("Tool").unwrap_or(struct_name);
    base.to_snake_case()
}

#[proc_macro_attribute]
pub fn agent_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as AgentToolArgs);
    let input = parse_macro_input!(item as ItemImpl);

    let struct_type = &input.self_ty;
    let struct_name = quote!(#struct_type).to_string();

    let tool_name = args.name.unwrap_or_else(|| derive_tool_name(&struct_name));

    let file_names = args.files.unwrap_or_else(|| vec![tool_name.clone()]);

    let definitions_body = if file_names.len() == 1 {
        let path = format!("tools/{}.md", file_names[0]);
        quote! {
            crate::tool::load_tool_definition_with_vars(&self.prompts, #path, &self.definition_vars())
                .into_iter()
                .collect()
        }
    } else {
        let stmts = file_names.iter().map(|f| {
            let path = format!("tools/{f}.md");
            quote! {
                if let Some(d) = crate::tool::load_tool_definition_with_vars(&self.prompts, #path, &self.definition_vars()) {
                    defs.push(d);
                }
            }
        });
        quote! {
            let mut defs = Vec::new();
            #(#stmts)*
            defs
        }
    };

    let user_items: Vec<&ImplItem> = input.items.iter().collect();

    let expanded = quote! {
        #[async_trait::async_trait]
        impl crate::tool::AgentTool for #struct_type {
            fn name(&self) -> &str {
                #tool_name
            }

            fn definitions(&self) -> Vec<crate::tool::ToolDefinition> {
                #definitions_body
            }

            #(#user_items)*
        }
    };

    expanded.into()
}
