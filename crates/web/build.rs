//! Reuse explicitly reviewed, host-independent IPC commands without linking Tauri.
//! An upstream signature/body change is type-checked in both hosts. This is an allowlist:
//! adding a desktop command never makes it remotely callable automatically.
use quote::{format_ident, quote, ToTokens};
use std::{collections::BTreeMap, env, fs, path::PathBuf};
use syn::{FnArg, Item, Pat, ReturnType};

fn main() {
    let root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../src-tauri/src/commands");
    let mut files = vec!["storage.rs".to_string(), "blocking.rs".to_string()];
    for name in [
        "activity",
        "avatars",
        "browse_history",
        "config",
        "database_maintenance",
        "favorites",
        "feed",
        "files",
        "friends",
        "game_log",
        "local_moderation",
        "local_player_moderations",
        "memos",
        "mutual_graph",
        "notifications",
        "player_list",
        "worlds",
    ] {
        files.push(format!("local/{name}.rs"));
    }
    for name in [
        "auth",
        "avatars",
        "economy",
        "favorites",
        "friends",
        "groups",
        "instances",
        "notifications",
        "search",
        "tools",
        "users",
        "worlds",
    ] {
        files.push(format!("vrchat/{name}/service.rs"));
        let types = format!("vrchat/{name}/types.rs");
        if root.join(&types).exists() {
            files.push(types);
        }
    }
    for name in [
        "friend_log",
        "frontend_batch",
        "group_calendar",
        "group_quick_moderation",
        "moderation_sync",
        "my_avatars",
        "notification_chains",
        "overlay_activity",
        "presence_automation",
        "quick_search",
        "realtime",
        "social_mutation",
        "translation",
        "user_dialog_tab_counts",
        "user_groups_overview",
        "vrc_status",
    ] {
        files.push(format!("application/{name}.rs"));
    }
    files.extend(["application/social_baseline/service.rs"].map(str::to_owned));
    println!("cargo:rerun-if-changed=commands.txt");
    let allowlist = fs::read_to_string("commands.txt").unwrap();
    let allowed: std::collections::BTreeSet<_> = allowlist
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect();
    let mut modules = BTreeMap::<Vec<String>, proc_macro2::TokenStream>::new();
    let mut routes = Vec::new();
    let mut names = Vec::new();
    for file in files {
        let path = root.join(&file);
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path).unwrap();
        let source = source
            .replace("use tauri::State;", "use crate::state::State;")
            .replace(
                "use tauri::{AppHandle, State};",
                "use crate::state::{AppHandle, State};",
            )
            .replace(
                "use tauri::{AppHandle, Manager, State};",
                "use crate::state::{AppHandle, State};",
            )
            .replace("tauri::async_runtime::", "tokio::task::")
            .replace("app.state::<AppState>()", "app.as_ref()");
        let mut parsed = syn::parse_file(&source).unwrap_or_else(|e| panic!("{file}: {e}"));
        let parts: Vec<String> = file
            .trim_end_matches(".rs")
            .split('/')
            .map(str::to_owned)
            .collect();
        let module_path: Vec<_> = parts.iter().map(|p| format_ident!("{p}")).collect();
        parsed
            .items
            .retain(|item| !matches!(item, Item::Mod(m) if m.ident == "tests"));
        parsed.items.retain(|item| {
            if let Item::Fn(f) = item {
                let command = f
                    .attrs
                    .iter()
                    .any(|a| a.path().to_token_stream().to_string() == "tauri :: command");
                return !command || allowed.contains(f.sig.ident.to_string().as_str());
            }
            true
        });
        for item in &mut parsed.items {
            if let Item::Fn(function) = item {
                let command = function
                    .attrs
                    .iter()
                    .any(|a| a.path().to_token_stream().to_string() == "tauri :: command");
                function.attrs.retain(|a| {
                    !["tauri", "specta"]
                        .iter()
                        .any(|p| a.path().segments.first().is_some_and(|s| s.ident == *p))
                });
                if !command {
                    continue;
                }
                let name = &function.sig.ident;
                let command_name = name.to_string();
                names.push(command_name.clone());
                let args: Vec<_> = function.sig.inputs.iter().map(|arg| {
                    let FnArg::Typed(arg) = arg else { panic!("unexpected receiver") };
                    let Pat::Ident(pat) = arg.pat.as_ref() else { panic!("unexpected argument") };
                    let key = pat.ident.to_string();
                    if key == "state" { return quote!(state.as_ref()); }
                    if key == "app" { return quote!(state.clone()); }
                    let mut words = key.split('_');
                    let mut camel = words.next().unwrap().to_string();
                    for word in words { let mut chars = word.chars(); if let Some(c) = chars.next() { camel.push(c.to_ascii_uppercase()); camel.extend(chars); } }
                    quote!(serde_json::from_value(args.get(#camel).cloned().unwrap_or(serde_json::Value::Null))?)
                }).collect();
                let mut call = quote!(#(#module_path)::* :: #name(#(#args),*));
                if function.sig.asyncness.is_some() {
                    call = quote!(#call.await);
                }
                if matches!(&function.sig.output, ReturnType::Type(_, ty) if ty.to_token_stream().to_string().starts_with("Result"))
                {
                    call = quote!(#call?);
                }
                routes.push(quote!(#command_name => Ok(serde_json::to_value(#call)?),));
            }
        }
        // Keep the original imports and DTOs scoped exactly as in the desktop host.
        let items = &parsed.items;
        modules.insert(parts, quote!(#(#items)*));
    }
    fn tree(
        prefix: &[String],
        modules: &BTreeMap<Vec<String>, proc_macro2::TokenStream>,
    ) -> proc_macro2::TokenStream {
        let mut children = std::collections::BTreeSet::new();
        for key in modules.keys() {
            if key.starts_with(prefix) && key.len() > prefix.len() {
                children.insert(key[prefix.len()].clone());
            }
        }
        let mut output = modules.get(prefix).cloned().unwrap_or_default();
        for child in children {
            let ident = format_ident!("{child}");
            let mut next = prefix.to_vec();
            next.push(child);
            let contents = tree(&next, modules);
            output.extend(quote!(pub(crate) mod #ident { #contents }));
        }
        output
    }
    for name in &allowed {
        assert!(
            names.iter().any(|n| n == name),
            "Allowlisted command disappeared: {name}"
        );
    }
    let modules = tree(&[], &modules);
    let generated = quote! {
        #modules
        pub const COMMANDS: &[&str] = &[#(#names),*];
        pub async fn dispatch(state: std::sync::Arc<crate::state::AppState>, command: &str, args: serde_json::Value) -> Result<serde_json::Value, crate::error::AppError> {
            match command { #(#routes)* _ => Err(crate::error::AppError::Custom(format!("Command is unavailable on this host: {command}"))) }
        }
    };
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(
        out.join("commands.rs"),
        prettyplease::unparse(&syn::parse2(generated).unwrap()),
    )
    .unwrap();
}
