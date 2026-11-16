//! The card a server tag opens: the server it's from, as Discord shows it,
//! with a way there when this account is in it.

use super::*;

pub(super) fn bind(window: &ChatterWindow, adapter: &Rc<RefCell<Adapter>>) {
    let card = window.global::<TagCard>();
    {
        let (state, weak) = (adapter.clone(), window.as_weak());
        card.on_show(move |guild, text, badge, x, y| {
            let Some(window) = weak.upgrade() else { return };
            let card = window.global::<TagCard>();
            let id = chatter_core::discord::server_id(&guild);
            {
                let adapter = state.borrow();
                let server = (!guild.is_empty())
                    .then(|| adapter.app.state.servers.iter().find(|server| server.id == id))
                    .flatten();
                card.set_name(server.map_or_else(SharedString::default, |server| server.name.as_str().into()));
                card.set_icon(server.and_then(|server| server.icon.as_deref()).unwrap_or_default().into());
                card.set_banner(server.and_then(|server| server.banner.as_deref()).unwrap_or_default().into());
                card.set_joined(server.is_some());
            }
            card.set_guild(id.as_str().into());
            card.set_raw(guild.clone());
            card.set_text(text);
            card.set_badge(badge);
            card.set_online(SharedString::default());
            card.set_members(SharedString::default());
            card.set_founded(SharedString::default());
            card.set_description(SharedString::default());
            card.set_failed(false);
            card.set_loading(!guild.is_empty());
            card.set_x(x);
            card.set_y(y);
            card.set_open(true);
            if !guild.is_empty() {
                state
                    .borrow_mut()
                    .action(&window, Action::ShowServerProfile(guild.to_string()));
            }
        });
    }
    let (state, weak) = (adapter.clone(), window.as_weak());
    card.on_go_to_server(move |id| {
        let Some(window) = weak.upgrade() else { return };
        window.global::<TagCard>().set_open(false);
        state
            .borrow_mut()
            .action(&window, Action::Navigate(Navigation::Server(id.into())));
    });
}

/// Fills the open card with what Discord's profile of the server says.
pub(super) fn show(adapter: &Adapter, window: &ChatterWindow) {
    let card = window.global::<TagCard>();
    if !card.get_open() || !card.get_loading() {
        return;
    }
    let Some((guild, result)) = &adapter.app.state.tag_server else {
        return;
    };
    if guild.as_str() != card.get_raw().as_str() {
        return;
    }
    card.set_loading(false);
    match result {
        Ok(server) => {
            if !server.name.is_empty() {
                card.set_name(server.name.as_str().into());
            }
            if let Some(icon) = &server.icon {
                card.set_icon(icon.as_str().into());
            }
            if let Some(banner) = &server.banner {
                card.set_banner(banner.as_str().into());
            }
            card.set_online(server.online.map(|count| count.to_string()).unwrap_or_default().into());
            card.set_members(server.members.map(|count| count.to_string()).unwrap_or_default().into());
            card.set_founded(server.founded.as_str().into());
            card.set_description(server.description.as_str().into());
        }
        Err(_) => card.set_failed(true),
    }
}
