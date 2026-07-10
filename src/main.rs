use serenity::{
    async_trait,
    model::gateway::Ready,
    prelude::*,
    model::id::{ChannelId, GuildId},
    model::channel::ChannelType,
    Client as SerenityClient, // Alias para diferenciar do reqwest::Client
};
use tokio::time::{sleep, Duration};
use reqwest::Client as ReqwestClient;
use rss::Channel;
use std::{collections::HashSet, env, fs, fs::File, io::BufReader};
use chrono::Local;
use serde_json::json;
use dotenv::dotenv;

const TARGET_GUILD_ID: u64 = 1523719297858277517;
const HISTORY_FILE: &str = "posted_news.json";
const CHECK_INTERVAL: u64 = 900; // 15 minutos

struct NewsSource {
    name: &'static str,
    url: &'static str,
}

const SOURCES: &[NewsSource] = &[
    NewsSource { name: "🟢 G1 Política (BR)", url: "https://g1.globo.com/rss/g1/politica/" },
    NewsSource { name: "🔴 CNN Brasil (BR)", url: "https://www.cnnbrasil.com.br/politica/feed/" },
    NewsSource { name: "🏛️ Nexo Jornal (BR)", url: "https://www.nexojornal.com.br/rss.xml" },
];

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} está online na versão SOVEREIGN POLITICS + GEMINI AI!", ready.user.name);
        
        let guild_id = GuildId(TARGET_GUILD_ID);
        
        let channels = guild_id.channels(&ctx.http).await.expect("Falha ao ler canais do servidor");
        
        let mut news_channel_id = None;
        let mut found_geral = false;
        let mut found_trabalho = false;
        let mut found_noticias = false;
        
        for (_, channel) in channels.iter() {
            if channel.name == "chat-geral" { found_geral = true; }
            if channel.name == "sala-de-trabalho" { found_trabalho = true; }
            if channel.name == "sovereign-noticias" { 
                found_noticias = true; 
                news_channel_id = Some(channel.id);
            }
        }
        
        if !found_geral {
            let _ = guild_id.create_channel(&ctx.http, |c| c.name("chat-geral").kind(ChannelType::Text)).await;
            println!("[+] Infraestrutura criada: #chat-geral");
        }
        if !found_trabalho {
            let _ = guild_id.create_channel(&ctx.http, |c| c.name("sala-de-trabalho").kind(ChannelType::Text)).await;
            println!("[+] Infraestrutura criada: #sala-de-trabalho");
        }
        if !found_noticias {
            if let Ok(c) = guild_id.create_channel(&ctx.http, |c| c.name("sovereign-noticias").kind(ChannelType::Text)).await {
                println!("[+] Infraestrutura criada: #sovereign-noticias");
                news_channel_id = Some(c.id);
            }
        }
        
        if let Some(nid) = news_channel_id {
            let ctx_clone = ctx.clone();
            tokio::spawn(async move {
                println!("[{}] Iniciando Varredura Política AI (GitHub Mode)...", Local::now().format("%Y-%m-%d %H:%M:%S"));
                fetch_and_post_news(&ctx_clone, nid).await;
                println!("Varredura concluída. Encerrando processo para o GitHub Actions.");
                std::process::exit(0);
            });
        } else {
            println!("[ERRO] Falha crítica ao mapear canal de notícias.");
        }
    }
}

fn load_history() -> HashSet<String> {
    if let Ok(file) = File::open(HISTORY_FILE) {
        let reader = BufReader::new(file);
        if let Ok(history) = serde_json::from_reader(reader) {
            return history;
        }
    }
    HashSet::new()
}

fn save_history(history: &HashSet<String>) {
    if let Ok(json) = serde_json::to_string(history) {
        let _ = fs::write(HISTORY_FILE, json);
    }
}

async fn rewrite_with_gemini(req_client: &ReqwestClient, title: &str, summary: &str) -> Option<String> {
    let api_key = env::var("GEMINI_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return None;
    }

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-pro-latest:generateContent?key={}", api_key);
    let prompt = format!(
        "Você é um jornalista político de elite brasileiro, totalmente isento e imparcial. \
        Reescreva a notícia a seguir para um post nas redes sociais (Instagram/X). \
        O texto deve ser direto, prender a atenção do leitor, não demonstrar viés ideológico e conter emojis pertinentes. \
        Evite opiniões, foque nos fatos de forma jornalística. \n\n\
        Título Original: {}\nResumo: {}", title, summary
    );

    let body = json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.3
        }
    });

    if let Ok(res) = req_client.post(&url).json(&body).send().await {
        if let Ok(json_resp) = res.json::<serde_json::Value>().await {
            if let Some(candidates) = json_resp.get("candidates") {
                if let Some(text) = candidates[0]["content"]["parts"][0]["text"].as_str() {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}

async fn fetch_and_post_news(ctx: &Context, channel_id: ChannelId) {
    let mut history = load_history();
    let req_client = ReqwestClient::builder()
        .user_agent("Mozilla/5.0 Sovereign Intel Bot")
        .build()
        .unwrap();

    for source in SOURCES {
        if let Ok(response) = req_client.get(source.url).send().await {
            if let Ok(bytes) = response.bytes().await {
                if let Ok(channel) = Channel::read_from(&bytes[..]) {
                    
                    for item in channel.items().iter().take(2) {
                        if let Some(link) = item.link() {
                            let link = link.to_string();
                            if !history.contains(&link) {
                                let title = item.title().unwrap_or("Sem Título").to_string();
                                let summary = item.description().unwrap_or("Sem Resumo").to_string();
                                
                                println!("[+] Nova Notícia Encontrada: {} -> Acionando Gemini...", title);
                                
                                // Extração de Imagem
                                let mut image_url = None;
                                if let Some(enclosure) = item.enclosure() {
                                    image_url = Some(enclosure.url().to_string());
                                } else if let Some(media) = item.extensions().get("media").and_then(|m| m.get("content")) {
                                    if let Some(content) = media.first() {
                                        if let Some(url) = content.attrs().get("url") {
                                            image_url = Some(url.to_string());
                                        }
                                    }
                                }
                                
                                // IA Engine
                                let clean_summary = summary.replace("<p>", "").replace("</p>", "").replace("<br>", "\n");
                                let final_text = match rewrite_with_gemini(&req_client, &title, &clean_summary).await {
                                    Some(ai_text) => ai_text,
                                    None => clean_summary, // Fallback se a IA falhar
                                };

                                let res = channel_id.send_message(&ctx.http, |m| {
                                    m.embed(|e| {
                                        let mut embed = e.title(format!("⚖️ {}", title))
                                         .url(&link)
                                         .description(&final_text)
                                         .color(0x00FF00) // Verde Brasileiro / Isento
                                         .author(|a| a.name(source.name))
                                         .footer(|f| f.text("🤖 IA de Monitoramento Político Imparcial | Sovereign Intel"));
                                         
                                        if let Some(img) = &image_url {
                                            embed = embed.image(img);
                                        }
                                        embed
                                    })
                                }).await;

                                if let Err(why) = res {
                                    println!("Erro ao enviar mensagem: {:?}", why);
                                } else {
                                    history.insert(link);
                                    save_history(&history);
                                    return; // Apenas 1 post por ciclo de 15m
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Removido o loop infinito para o GitHub Actions

#[tokio::main]
async fn main() {
    dotenv().ok();
    
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILDS;

    let mut client = SerenityClient::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
