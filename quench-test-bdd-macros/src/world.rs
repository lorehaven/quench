use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Expr, parse2};

pub fn expand(input: TokenStream) -> TokenStream {
    match try_expand(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn try_expand(input: TokenStream) -> syn::Result<TokenStream> {
    let derive: DeriveInput = parse2(input)?;
    let name = &derive.ident;
    let (impl_generics, type_generics, where_clause) = derive.generics.split_for_impl();

    let mut init: Option<Expr> = None;
    for attribute in &derive.attrs {
        if attribute.path().is_ident("world") {
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("init") {
                    init = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported option (use `init = path`)"))
                }
            })?;
        }
    }
    let build = init.map_or_else(
        || quote!(<Self as ::core::default::Default>::default()),
        |path| quote!((#path)().await),
    );
    Ok(quote! {
        impl #impl_generics ::quench_test_bdd::World for #name #type_generics #where_clause {
            fn new() -> ::quench_test_bdd::BoxFuture<'static, Self> {
                ::std::boxed::Box::pin(async { #build })
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    #[test]
    fn world_uses_the_init_function_or_default() {
        let with = expand(quote!(
            #[world(init = Self::new)]
            struct W {
                a: u8,
            }
        ))
        .to_string();
        assert!(
            with.contains("impl :: quench_test_bdd :: World for W")
                && with.contains("(Self :: new) () . await"),
            "{with}"
        );
        let without = expand(quote!(
            struct W<T>
            where
                T: Default,
            {
                a: T,
            }
        ))
        .to_string();
        assert!(
            without.contains(":: default ()") && without.contains("where T : Default"),
            "{without}"
        );
    }

    #[test]
    fn bad_options_are_compile_errors() {
        assert!(
            expand(quote!(
                #[world(nope = 1)]
                struct W;
            ))
            .to_string()
            .contains("compile_error")
        );
        assert!(
            expand(quote!(
                fn not_a_type() {}
            ))
            .to_string()
            .contains("compile_error")
        );
    }
}
