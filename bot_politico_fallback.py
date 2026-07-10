import discord
import feedparser
import asyncio
import os
import json
import requests
import re
from datetime import datetime
from PIL import Image, ImageDraw, ImageFont
import io
import textwrap

# Setup inicial do Discord
TOKEN = os.getenv("DISCORD_TOKEN")
HISTORY_FILE = "posted_news.json"
TARGET_CHANNEL_ID = 1523719297858277517

# Fontes imparciais, renomadas e seguras focadas em política internacional e nacional
NEWS_SOURCES = {
    "📰 Reuters (Global)": "https://www.reutersagency.com/feed/?taxonomy=best-sectors&post_type=best",
    "🌍 BBC News (World)": "http://feeds.bbci.co.uk/news/world/rss.xml",
    "🏛️ Nexo Jornal (BR)": "https://www.nexojornal.com.br/rss.xml"
}

intents = discord.Intents.default()
client = discord.Client(intents=intents)

def load_history():
    if os.path.exists(HISTORY_FILE):
        try:
            with open(HISTORY_FILE, "r") as f:
                return set(json.load(f))
        except: return set()
    return set()

def save_history(history):
    with open(HISTORY_FILE, "w") as f:
        json.dump(list(history), f)

def normalize_url(url):
    if not url: return ""
    return url.lower().split("?")[0].rstrip("/")

def create_intel_image(title, source):
    """Gera uma imagem elegante, embaçada/dark, com a notícia escrita"""
    # Imagem base escura estilo "Sovereign/Cyberpunk"
    width, height = 800, 400
    img = Image.new('RGB', (width, height), color=(18, 18, 20))
    draw = ImageDraw.Draw(img)
    
    # Textos da Imagem
    wrapped_title = textwrap.fill(title, width=50)
    
    try:
        # Se você tiver a fonte instalada no Linux, melhor. Usaremos default por enquanto.
        font_title = ImageFont.truetype("/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", 24)
        font_source = ImageFont.truetype("/usr/share/fonts/TTF/DejaVuSans.ttf", 18)
    except:
        font_title = ImageFont.load_default()
        font_source = ImageFont.load_default()

    draw.text((40, 50), "SOVEREIGN INTEL - ALERTA POLÍTICO", fill=(120, 0, 255), font=font_source)
    draw.text((40, 100), wrapped_title, fill=(230, 230, 230), font=font_title)
    draw.text((40, height - 50), f"FONTE VALIDADA: {source}", fill=(100, 100, 100), font=font_source)

    # Converte imagem em bytes para o Discord
    img_byte_arr = io.BytesIO()
    img.save(img_byte_arr, format='PNG')
    img_byte_arr.seek(0)
    return img_byte_arr

async def verify_and_post():
    await client.wait_until_ready()
    channel = client.get_channel(TARGET_CHANNEL_ID)
    if not channel:
        print(f"[Erro] Canal {TARGET_CHANNEL_ID} não encontrado.")
        return

    history = load_history()
    
    for source_name, url in NEWS_SOURCES.items():
        try:
            headers = {'User-Agent': 'Mozilla/5.0'}
            response = requests.get(url, headers=headers, timeout=15)
            feed = feedparser.parse(response.content)
            
            for entry in feed.entries[:2]: # Tenta as 2 mais recentes
                link = normalize_url(entry.link)
                if link in history: continue
                
                title = entry.title
                summary = getattr(entry, 'summary', '')
                
                # Filtro primitivo de IA/Alucinação (Remove HTML, junta texto)
                clean_summary = re.sub('<[^<]+>', '', summary)
                clean_summary = clean_summary[:500] + "..." if len(clean_summary) > 500 else clean_summary
                
                print(f"[+] Notícia Inédita: {title}")
                
                # Formatação Imparcial e Elegante
                embed = discord.Embed(
                    title=f"⚖️ {title}",
                    url=entry.link,
                    description=f"**Resumo Estratégico:**\n{clean_summary}",
                    color=0x7800FF # Purple Sovereign
                )
                embed.set_author(name=source_name)
                embed.set_footer(text="🤖 Monitoramento Imparcial Automático")
                
                # Gera Imagem Dinâmica
                img_bytes = create_intel_image(title, source_name)
                discord_file = discord.File(fp=img_bytes, filename="news_banner.png")
                embed.set_image(url="attachment://news_banner.png")

                await channel.send(embed=embed, file=discord_file)
                
                history.add(link)
                save_history(history)
                return # Posta apenas 1 notícia por ciclo (15 min) para não flodar
                
        except Exception as e:
            print(f"[Erro] Falha ao processar {source_name}: {e}")

async def background_loop():
    while not client.is_closed():
        print(f"[{datetime.now()}] Iniciando Varredura Política...")
        await verify_and_post()
        # Aguarda 15 minutos (900 segundos)
        await asyncio.sleep(900)

@client.event
async def on_ready():
    print(f"Sovereign Politics Bot Logado como {client.user}")
    client.loop.create_task(background_loop())

if __name__ == "__main__":
    if not TOKEN:
        print("Erro: DISCORD_TOKEN não encontrado.")
    else:
        client.run(TOKEN)
