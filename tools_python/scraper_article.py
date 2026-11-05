import sys
import re
import json
import html
from urllib.parse import urlparse
import requests

# Forcer la sortie standard et d'erreur en UTF-8 strict (indispensable sous Windows)
try:
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    if hasattr(sys.stderr, 'reconfigure'):
        sys.stderr.reconfigure(encoding='utf-8')
except Exception:
    pass

def extract_meta_content(html_text, attr_name, attr_val):
    pattern = rf'<meta\s+[^>]*{attr_name}=["\']{re.escape(attr_val)}["\'][^>]*content=["\'](.*?)["\']'
    m = re.search(pattern, html_text, re.IGNORECASE | re.DOTALL)
    if not m:
        pattern2 = rf'<meta\s+[^>]*content=["\'](.*?)["\'][^>]*{attr_name}=["\']{re.escape(attr_val)}["\']'
        m = re.search(pattern2, html_text, re.IGNORECASE | re.DOTALL)
    if m:
        return html.unescape(m.group(1)).strip()
    return ""

def clean_html(raw_html):
    cleaned = re.sub(r'<script[^>]*>.*?</script>', '', raw_html, flags=re.DOTALL | re.IGNORECASE)
    cleaned = re.sub(r'<style[^>]*>.*?</style>', '', cleaned, flags=re.DOTALL | re.IGNORECASE)
    cleaned = re.sub(r'<[^>]+>', ' ', cleaned)
    cleaned = html.unescape(cleaned)
    lines = [line.strip() for line in cleaned.splitlines()]
    return '\n'.join(line for line in lines if line)

def scrape_article(url):
    result = {
        "success": False,
        "url": url,
        "title": "",
        "source": "",
        "date": "",
        "text": "",
        "image_url": "",
        "error": ""
    }

    url = url.strip()
    if not url.startswith("http://") and not url.startswith("https://"):
        url = "https://" + url

    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0",
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        "Accept-Language": "fr-FR,fr;q=0.9,en-US;q=0.8,en;q=0.7",
    }

    try:
        resp = requests.get(url, headers=headers, timeout=12, allow_redirects=True)
        # Décodage prioritaire UTF-8 pour préserver les accents français
        if resp.encoding and "iso" in resp.encoding.lower():
            raw_html = resp.text
        else:
            raw_html = resp.content.decode("utf-8", errors="replace")
    except Exception as e:
        result["error"] = str(e)
        return result

    # 1. Source / Nom du média
    domain = urlparse(url).netloc.replace("www.", "")
    source_name = extract_meta_content(raw_html, "property", "og:site_name")
    if not source_name:
        source_name = extract_meta_content(raw_html, "name", "twitter:site")
    if not source_name or len(source_name) < 2:
        parts = domain.split('.')
        source_name = parts[0].upper() if parts else domain
    result["source"] = source_name.strip()

    # 2. Titre
    title = extract_meta_content(raw_html, "property", "og:title")
    if not title:
        title = extract_meta_content(raw_html, "name", "twitter:title")
    if not title:
        m = re.search(r'<title[^>]*>(.*?)</title>', raw_html, re.IGNORECASE | re.DOTALL)
        if m:
            title = html.unescape(m.group(1)).strip()
    # Nettoyer les suffixes comme " - Midi Libre" ou " | Le Monde.fr"
    if title:
        title = re.sub(r'\s*[-|–—]\s*(' + re.escape(result["source"]) + r'|.*?\.(fr|com|org|net)).*?$', '', title, flags=re.IGNORECASE).strip()
    result["title"] = title

    # 3. Image d'illustration
    img = extract_meta_content(raw_html, "property", "og:image")
    if not img:
        img = extract_meta_content(raw_html, "name", "twitter:image")
    result["image_url"] = img or ""

    # 4. Date de publication
    date = extract_meta_content(raw_html, "property", "article:published_time")
    if not date:
        date = extract_meta_content(raw_html, "name", "date")
    if not date:
        m = re.search(r'<time[^>]*datetime=["\'](.*?)["\']', raw_html, re.IGNORECASE)
        if m:
            date = m.group(1)
    if not date:
        # Chercher dans l'URL (ex: /2026/11/03/)
        m_url = re.search(r'/(\d{4})/(\d{2})/(\d{2})/', url)
        if m_url:
            date = f"{m_url.group(3)}/{m_url.group(2)}/{m_url.group(1)}"
    else:
        m_date = re.search(r'(\d{4})-(\d{2})-(\d{2})', date)
        if m_date:
            date = f"{m_date.group(3)}/{m_date.group(2)}/{m_date.group(1)}"
    result["date"] = date or ""

    # 5. Description / Résumé / Texte de l'article
    desc = extract_meta_content(raw_html, "property", "og:description")
    if not desc:
        desc = extract_meta_content(raw_html, "name", "description")
    if not desc:
        desc = extract_meta_content(raw_html, "name", "twitter:description")

    # Extraire les paragraphes propres
    paragraphs = []
    article_m = re.search(r'<article[^>]*>(.*?)</article>', raw_html, re.IGNORECASE | re.DOTALL)
    scope = article_m.group(1) if article_m else raw_html

    for p in re.findall(r'<p[^>]*>(.*?)</p>', scope, re.IGNORECASE | re.DOTALL):
        cleaned_p = clean_html(p)
        cleaned_lower = cleaned_p.lower()
        if len(cleaned_p) > 60 and not any(bad in cleaned_lower for bad in [
            "cookie", "abonnez-vous", "newsletter", "publicité", "tous droits réservés",
            "ajouter aux sources", "lire aussi", "en savoir plus"
        ]):
            paragraphs.append(cleaned_p)
            if len(paragraphs) >= 5:
                break

    full_text = ""
    if desc and len(desc) > 30:
        full_text += desc + "\n\n"
    if paragraphs:
        for p in paragraphs:
            # Éviter doublon si la description reprend le 1er paragraphe
            if p[:35].lower() not in full_text.lower():
                full_text += p + "\n\n"

    result["text"] = full_text.strip()
    result["success"] = bool(result["title"] or result["text"])
    return result

if __name__ == "__main__":
    if len(sys.argv) > 1:
        url = sys.argv[1]
        data = scrape_article(url)
        print(json.dumps(data, ensure_ascii=False, indent=2))
    else:
        print(json.dumps({"error": "No URL specified"}, ensure_ascii=False))
