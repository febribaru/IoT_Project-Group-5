import re

with open('script.js', 'r', encoding='utf-8') as f:
    code = f.read()

# remove comments
code_clean = re.sub(r'//.*', '', code)
code_clean = re.sub(r'/\*[\s\S]*?\*/', '', code_clean)

# replace string contents to avoid parens/braces inside strings
def clean_str(match):
    return '""'
code_clean = re.sub(r'"(?:\\.|[^"\\])*"', clean_str, code_clean)
code_clean = re.sub(r"'(?:\\.|[^'\\])*'", clean_str, code_clean)
code_clean = re.sub(r"`(?:\\.|[^`\\])*`", clean_str, code_clean)

# remove known regexes like /\s*\([^)]*\)/g
code_clean = re.sub(r'/[^/\n]+/g?', '""', code_clean)

p, b, bk = 0, 0, 0
err = False
lines = code_clean.split('\n')
for line_no, line in enumerate(lines, 1):
    for ch in line:
        if ch == '(': p += 1
        elif ch == ')':
            p -= 1
            if p < 0:
                print(f"Negative parens at line {line_no}: {line}")
                err = True
                break
        elif ch == '{': b += 1
        elif ch == '}':
            b -= 1
            if b < 0:
                print(f"Negative braces at line {line_no}: {line}")
                err = True
                break
        elif ch == '[': bk += 1
        elif ch == ']':
            bk -= 1
            if bk < 0:
                print(f"Negative brackets at line {line_no}: {line}")
                err = True
                break
    if err:
        break

print(f"Result -> parens: {p}, braces: {b}, brackets: {bk}")

