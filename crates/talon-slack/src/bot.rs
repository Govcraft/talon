use std::sync::Arc;

use slack_morphism::prelude::*;
use talon_channel_sdk::GatewayClient;
use tokio::sync::Mutex;

/// Shared state passed to Slack event handlers via `SlackClientEventsUserState`.
struct BotState {
    gateway: Arc<Mutex<GatewayClient>>,
    bot_token: SlackApiToken,
}

/// Run the Slack bot event loop using Socket Mode.
pub async fn run(
    gateway: Arc<Mutex<GatewayClient>>,
    bot_token: String,
    app_token: String,
) -> anyhow::Result<()> {
    let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

    let bot_token_value: SlackApiTokenValue = bot_token.into();
    let bot_api_token = SlackApiToken::new(bot_token_value);

    let state = BotState {
        gateway,
        bot_token: bot_api_token,
    };

    let socket_mode_callbacks =
        SlackSocketModeListenerCallbacks::new().with_push_events(handle_push_events);

    let listener_environment = Arc::new(
        SlackClientEventsListenerEnvironment::new(client.clone())
            .with_user_state(state)
            .with_error_handler(error_handler),
    );

    let socket_mode_listener = SlackClientSocketModeListener::new(
        &SlackClientSocketModeConfig::new(),
        listener_environment,
        socket_mode_callbacks,
    );

    let app_token_value: SlackApiTokenValue = app_token.into();
    let app_api_token = SlackApiToken::new(app_token_value);

    socket_mode_listener.listen_for(&app_api_token).await?;

    tracing::info!("Slack bot is listening for events");

    socket_mode_listener.serve().await;

    Ok(())
}

/// Handle push events received from Slack Socket Mode.
async fn handle_push_events(
    event: SlackPushEventCallback,
    client: Arc<SlackHyperClient>,
    states: SlackClientEventsUserState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let SlackEventCallbackBody::Message(msg_event) = event.event else {
        return Ok(());
    };

    // Skip bot messages to avoid loops
    if msg_event.sender.bot_id.is_some() {
        return Ok(());
    }

    let text = match msg_event.content.as_ref().and_then(|c| c.text.as_ref()) {
        Some(text) => text.clone(),
        None => return Ok(()),
    };

    let sender_id = msg_event
        .sender
        .user
        .as_ref()
        .map(|u| format!("sl:{u}"))
        .unwrap_or_else(|| "sl:unknown".to_string());

    let channel_id = match msg_event.origin.channel.as_ref() {
        Some(ch) => ch.clone(),
        None => return Ok(()),
    };

    tracing::debug!(sender_id = %sender_id, channel = %channel_id, "Received message from Slack");

    let states_read = states.read().await;
    let bot_state = states_read
        .get_user_state::<BotState>()
        .expect("BotState must be registered");

    // Forward to gateway
    let response_text = {
        let mut gw = bot_state.gateway.lock().await;
        match gw.send_message(&sender_id, &text, None).await {
            Ok(response) => response.text,
            Err(e) => {
                tracing::error!("Gateway error: {e}");
                "Sorry, I encountered an error processing your message.".to_string()
            }
        }
    };

    let bot_token = bot_state.bot_token.clone();
    drop(states_read);

    // Send response back to Slack
    let session = client.open_session(&bot_token);
    session
        .chat_post_message(&SlackApiChatPostMessageRequest::new(
            channel_id,
            SlackMessageContent::new().with_text(response_text),
        ))
        .await?;

    Ok(())
}

fn error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    tracing::error!("Slack listener error: {err}");
    HttpStatusCode::OK
}
