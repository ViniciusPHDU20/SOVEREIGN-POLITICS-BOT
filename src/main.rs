use serenity::{
    async_trait,
    model::gateway::Ready,
    prelude::*,
    model::id::{ChannelId, GuildId},
    model::channel::ChannelType,
    Client as SerenityClient, // Alias para diferenciar do reqwest::Client
};
use tokio;
use reqwest::Client as ReqwestClient;
use rss::Channel;
use std::{collections::HashSet, env, fs, fs::File, io::BufReader};
use chrono::Local;
use serde_json::json;
use dotenv::dotenv;
use rand::seq::SliceRandom;

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
        println!("[ERRO] GEMINI_API_KEY não encontrada no ambiente.");
        return None;
    }

    // Usando gemini-flash-latest que sempre aponta para o motor livre e atualizado do Google
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:generateContent?key={}", api_key);
    
    let prompt = format!(
        "Você é um renomado jornalista investigativo e analista político brasileiro. \
        Seu objetivo é pegar notícias e escrever comentários afiados, analíticos e profundos nas suas redes sociais (X/Instagram). \
        Não aja como um robô resumidor. Seja humano, orgânico e intelectual. \
        **OBRIGAÇÃO CRÍTICA:** Você DEVE escrever um texto longo e denso. Você é OBRIGADO a desenvolver no mínimo 3 parágrafos robustos dissecando o impacto real que o fato traz para a sociedade, economia ou política. \
        Faça uma análise crítica e reflexiva, mantendo a imparcialidade jornalística e baseando-se estritamente na verdade dos fatos fornecidos. \
        NUNCA responda com apenas uma frase. Se a notícia for curta, crie um contexto analítico profundo em torno das implicações dela.\n\n\
        Notícia a ser dissecada:\n\
        Título: {}\nResumo: {}", title, summary
    );

    let body = json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.8
        }
    });

    let res = req_client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    if let Ok(response) = res {
        if let Ok(json) = response.json::<serde_json::Value>().await {
            if let Some(candidates) = json.get("candidates") {
                if let Some(first) = candidates.get(0) {
                    if let Some(content) = first.get("content") {
                        if let Some(parts) = content.get("parts") {
                            if let Some(first_part) = parts.get(0) {
                                if let Some(text) = first_part.get("text") {
                                    return Some(text.as_str().unwrap_or("").to_string());
                                }
                            }
                        }
                    }
                }
            } else {
                println!("[ERRO-GEMINI] Resposta inválida ou sem candidatos: {:?}", json);
            }
        } else {
            println!("[ERRO-GEMINI] Falha ao decodificar JSON da API.");
        }
    } else {
        println!("[ERRO-GEMINI] Falha ao enviar request HTTP.");
    }

    None
}

async fn fetch_and_post_news(ctx: &Context, channel_id: ChannelId) {
    let mut history = load_history();
    let req_client = ReqwestClient::builder()
        .user_agent("Mozilla/5.0 Sovereign Intel Bot")
        .build()
        .unwrap();

    struct NewsItem {
        title: String,
        link: String,
        summary: String,
        image_url: Option<String>,
        source_name: String,
    }

    let mut all_news = Vec::new();

    for source in SOURCES {
        if let Ok(response) = req_client.get(source.url).send().await {
            if let Ok(bytes) = response.bytes().await {
                if let Ok(channel) = Channel::read_from(&bytes[..]) {
                    for item in channel.items().iter().take(5) { // Puxa as 5 mais recentes
                        if let Some(link) = item.link() {
                            let link_str = link.to_string();
                            if !history.contains(&link_str) {
                                let title = item.title().unwrap_or("Sem Título").to_string();
                                let summary = item.description().unwrap_or("Sem Resumo").to_string();
                                
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
                                
                                all_news.push(NewsItem {
                                    title, link: link_str, summary, image_url, source_name: source.name.to_string()
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if all_news.is_empty() {
        println!("[*] Nenhuma notícia nova para curadoria humana neste ciclo.");
        return;
    }

    // Curadoria Humana Aleatória: Escolhe apenas 1 notícia entre todas as fontes
    {
        let mut rng = rand::thread_rng();
        all_news.shuffle(&mut rng);
    }
    let chosen = all_news.remove(0);

    println!("[+] Curadoria Humana escolheu: {} -> Acionando Gemini...", chosen.title);

    let clean_summary = chosen.summary.replace("<p>", "").replace("</p>", "").replace("<br>", "\n");
    let final_text = match rewrite_with_gemini(&req_client, &chosen.title, &clean_summary).await {
        Some(ai_text) => ai_text,
        None => clean_summary,
    };

    let mut display_title = format!("🖋️ {}", chosen.title);
    if display_title.len() > 250 {
        display_title.truncate(247);
        display_title.push_str("...");
    }

    let mut display_text = final_text.clone();
    if display_text.len() > 4000 {
        display_text.truncate(3997);
        display_text.push_str("...");
    }

    let res = channel_id.send_message(&ctx.http, |m| {
        m.embed(|e| {
            let mut embed = e.title(display_title)
             .url(&chosen.link)
             .description(&display_text)
             .color(0x000000) // Preto Clássico (Jornal)
             .author(|a| a.name(&chosen.source_name))
             .footer(|f| f.text("📝 Análise Independente | Sovereign Intel"));
             
            if let Some(img) = &chosen.image_url {
                embed = embed.image(img);
            }
            embed
        })
    }).await;

    if let Err(why) = res {
        println!("Erro ao enviar mensagem: {:?}", why);
    } else {
        history.insert(chosen.link.clone());
        save_history(&history);
        println!("[OK] Post orgânico realizado com sucesso.");
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
