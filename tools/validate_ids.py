import re

with open('script.js', 'r', encoding='utf-8', errors='ignore') as f:
    js = f.read()

with open('index.html', 'r', encoding='utf-8', errors='ignore') as f:
    html = f.read()

js_ids = set(re.findall(r'getElementById\([\'"]([^\'"]+)[\'"]\)', js))
html_ids = set(re.findall(r'id=[\'"]([^\'"]+)[\'"]', html))

print(f"Total IDs queried in script.js: {len(js_ids)}")
print(f"Total IDs found in index.html: {len(html_ids)}")

missing_in_html = js_ids - html_ids
print("IDs in JS but missing in HTML:")
for m in sorted(missing_in_html):
    print("  -", m)

