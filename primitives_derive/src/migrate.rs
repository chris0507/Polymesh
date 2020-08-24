use syn::export::TokenStream2;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned as _;
use syn::{visit_mut, Data, DataEnum, DataStruct, DeriveInput, Ident, Index, Type};

/// Appends `Old` suffix to the `ident` returning the new appended `Ident`.
fn oldify_ident(ident: &Ident) -> Ident {
    Ident::new(&format!("{}Old", ident), ident.span())
}

/// Appends `Old` suffix to those identifiers in `ty` as instructed by `refs`.
fn oldify_type(ty: &mut Type, refs: &Option<MigrateRefs>) {
    let refs = match refs {
        None => return,
        Some(refs) => refs,
    };
    struct Vis<'a>(&'a MigrateRefs);
    impl visit_mut::VisitMut for Vis<'_> {
        fn visit_ident_mut(&mut self, ident: &mut Ident) {
            match &self.0 {
                // Got `#[migrate]`; instructed oldify any identifier.
                MigrateRefs::Any => {}
                // Got `#[migrate(TypeA, TypeB, ...)]`, so check if `ident` is one of those.
                MigrateRefs::Listed(list)
                    if {
                        let ident = ident.to_string();
                        list.iter().any(|elem| elem == &ident)
                    } => {}
                _ => return,
            }
            *ident = oldify_ident(ident);
        }
    }
    visit_mut::visit_type_mut(&mut Vis(&refs), ty);
}

/// Go over the given `fields`,
/// stripping any `#[migrate]` attributes on them (while noting them), stripping those,
/// and then extending the fields in the output with the presence of `#[migrate]`.
fn fields_with_migration(fields: &mut syn::Fields) -> Vec<(bool, &syn::Field)> {
    let mut fields_vec = Vec::with_capacity(fields.len());
    for f in fields.iter_mut() {
        let refs = extract_migrate_refs(&mut f.attrs);
        oldify_type(&mut f.ty, &refs);
        fields_vec.push((refs.is_some(), &*f));
    }
    fields_vec
}

/// Quote the given `fields`, assumed to be a variant/product,
/// into a pair of a destructuring (unpacking) pattern
/// and a piece of a struct/variant initialization expression.
fn quote_pack_unpack(fields: &[(bool, &syn::Field)]) -> (Vec<TokenStream2>, Vec<TokenStream2>) {
    fn pack(migrate: &bool, field: impl quote::ToTokens, var: &Ident) -> TokenStream2 {
        match migrate {
            true => quote!( #field: #var.migrate()? ),
            false => quote!( #field: #var ),
        }
    }
    fields
        .iter()
        .enumerate()
        .map(|(index, (migrate, field))| match &field.ident {
            Some(ident) => (quote!( #ident ), pack(migrate, &ident, &ident)),
            None => {
                let span = field.ty.span();
                let index = index as u32;
                let idx = Index { index, span };
                let var = Ident::new(&format!("idx{}", index), span);
                (quote!( #idx: #var ), pack(migrate, idx, &var))
            }
        })
        .unzip()
}

/// Implements `#[derive(Migrate)]`.
pub(crate) fn impl_migrate(mut input: DeriveInput) -> TokenStream2 {
    let name = input.ident;
    input.ident = oldify_ident(&name);
    let old_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let migration = match &mut input.data {
        Data::Union(_) => {
            return quote! {
                compile_error!("cannot derive `Migrate` for unions");
            }
        }
        Data::Struct(DataStruct { ref mut fields, .. }) => {
            // Handle `#[migrate]`s and old-ifying types
            // and then interpolate unpacking & packing.
            let (unpack, pack) = quote_pack_unpack(&fields_with_migration(fields));
            quote!( match self { Self { #(#unpack,)* } => Self::Into { #(#pack,)* } } )
        }
        Data::Enum(DataEnum {
            ref mut variants, ..
        }) => {
            // Same for each variant, as-if it were a struct.
            let arm = variants
                .iter_mut()
                .map(|syn::Variant { ident, fields, .. }| {
                    let (unpack, pack) = quote_pack_unpack(&fields_with_migration(fields));
                    quote!( Self::#ident { #(#unpack,)* } => Self::Into::#ident { #(#pack,)* } )
                });
            quote!( match self { #(#arm,)* } )
        }
    };

    quote! {
        #[derive(::codec::Decode)]
        #input

        impl #impl_generics polymesh_primitives::migrate::Migrate
        for #old_name #ty_generics
        #where_clause {
            type Into = #name #ty_generics;
            fn migrate(self) -> Option<Self::Into> { Some(#migration) }
        }
    }
}

/// Semantic representation of the `#[migrate]` attribute.
enum MigrateRefs {
    /// Derived from `#[migrate]`.
    /// Any identifier in the type of the field should be migrated.
    Any,
    /// Derived from `#[migrate(ident, ident, ...)]`.
    /// Only those identifiers in the list and which match in the type of the field should be migrated.
    Listed(Vec<syn::Ident>),
}

/// Returns information about any `#[migrate]` attributes while also stripping them.
///
/// The form `#[migrate = ".."]` does qualify.
fn extract_migrate_refs(attrs: &mut Vec<syn::Attribute>) -> Option<MigrateRefs> {
    let mut mig_ref = None;
    attrs.retain(|attr| {
        // Only care about `migrate], and remove all of those, irrespective of form.
        if attr.path.is_ident("migrate") {
            if attr.tokens.is_empty() {
                // Got exactly `#[migrate]`.
                // User doesn't wish to specify which types to migrate, so assume all.
                mig_ref = Some(MigrateRefs::Any);
            } else if let Ok(refs) = attr.parse_args_with(|ps: ParseStream| {
                // Got `migrate(ident, ident, ...)`.
                // User only wants to oldify the given identifiers.
                // Applies in e.g., `field: Vec<Foo>` where `Foo` is being migrated
                // but `Vec` shouldn't be renamed as it is a container of `Foo`s.
                ps.parse_terminated::<_, syn::Token![,]>(Ident::parse)
                    .map(|iter| iter.into_iter().collect())
            }) {
                mig_ref = Some(MigrateRefs::Listed(refs));
            }
            false
        } else {
            true
        }
    });
    mig_ref
}
