use osl_privacy_hub::visible_osl_mark::{
    put_visible_osl_mark, take_visible_osl_mark_out, VisibleOslMarkAppView, VisibleOslMarkRequest,
    VisibleOslMarkState,
};

fn usage() -> ! {
    eprintln!(
        "usage: task_0284_visible_osl_mark <put|remove> --app-view discord-profile-name --name <name> --mark-state <visible|silent>"
    );
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 7
        || args[1] != "--app-view"
        || args[3] != "--name"
        || args[5] != "--mark-state"
    {
        usage();
    }

    let command = args[0].as_str();
    let app_view = parse_app_view(&args[2]).unwrap_or_else(|| usage());
    let name = args[4].clone();
    let mark_state = parse_mark_state(&args[6]).unwrap_or_else(|| usage());
    let request = VisibleOslMarkRequest::new(app_view, name, mark_state);

    let result = match command {
        "put" => put_visible_osl_mark(&request),
        "remove" => take_visible_osl_mark_out(&request),
        _ => usage(),
    }
    .unwrap_or_else(|error| {
        eprintln!("TASK0284_ERROR={error}");
        std::process::exit(1);
    });

    println!("TASK0284_DIRECT_COMMAND={command}");
    println!("TASK0284_APP_VIEW={}", result.app_view.as_str());
    println!("TASK0284_MARK_STATE={}", result.mark_state.as_str());
    println!("TASK0284_MARK_STRING={}", result.mark);
    println!("TASK0284_BEFORE={}", result.before);
    println!("TASK0284_AFTER={}", result.after);
    println!("TASK0284_CHANGED={}", result.changed);
}

fn parse_app_view(value: &str) -> Option<VisibleOslMarkAppView> {
    match value {
        "discord-profile-name" => Some(VisibleOslMarkAppView::DiscordProfileName),
        _ => None,
    }
}

fn parse_mark_state(value: &str) -> Option<VisibleOslMarkState> {
    match value {
        "visible" => Some(VisibleOslMarkState::Visible),
        "silent" => Some(VisibleOslMarkState::Silent),
        _ => None,
    }
}
