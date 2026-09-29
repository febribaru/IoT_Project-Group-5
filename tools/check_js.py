with open('script.js', 'r', encoding='utf-8') as f:
    code = f.read()

lines = code.split('\n')
p_stack = []
b_stack = []
k_stack = []

in_single_str = False
in_double_str = False
in_backtick_str = False
in_line_comment = False
in_block_comment = False

for line_idx, line in enumerate(lines, 1):
    i = 0
    while i < len(line):
        ch = line[i]
        nxt = line[i+1] if i + 1 < len(line) else ''

        if in_line_comment:
            break
        if in_block_comment:
            if ch == '*' and nxt == '/':
                in_block_comment = False
                i += 2
                continue
            i += 1
            continue

        if not (in_single_str or in_double_str or in_backtick_str):
            if ch == '/' and nxt == '/':
                in_line_comment = True
                break
            if ch == '/' and nxt == '*':
                in_block_comment = True
                i += 2
                continue
            if ch == '/' and (i == 0 or line[i-1] in '(=,:[!&|?'):
                # regex literal
                j = i + 1
                while j < len(line):
                    if line[j] == '\\':
                        j += 2
                    elif line[j] == '/':
                        j += 1
                        while j < len(line) and line[j] in 'gimsuy':
                            j += 1
                        break
                    else:
                        j += 1
                i = j
                continue

        if ch == '\\':
            i += 2
            continue

        if ch == "'" and not in_double_str and not in_backtick_str:
            in_single_str = not in_single_str
        elif ch == '"' and not in_single_str and not in_backtick_str:
            in_double_str = not in_double_str
        elif ch == '`' and not in_single_str and not in_double_str:
            in_backtick_str = not in_backtick_str

        if not (in_single_str or in_double_str or in_backtick_str):
            if ch == '(':
                p_stack.append((line_idx, ch))
            elif ch == ')':
                if not p_stack:
                    print(f'Mismatched ) at line {line_idx}: {line}')
                else:
                    p_stack.pop()
            elif ch == '{':
                b_stack.append((line_idx, ch))
            elif ch == '}':
                if not b_stack:
                    print(f'Mismatched }} at line {line_idx}: {line}')
                else:
                    b_stack.pop()
            elif ch == '[':
                k_stack.append((line_idx, ch))
            elif ch == ']':
                if not k_stack:
                    print(f'Mismatched ] at line {line_idx}: {line}')
                else:
                    k_stack.pop()
        i += 1
    in_line_comment = False

print(f'Unclosed ( count: {len(p_stack)}')
print(f'Unclosed {{ count: {len(b_stack)}')
print(f'Unclosed [ count: {len(k_stack)}')
if p_stack:
    print('First unclosed ( at lines:', [x[0] for x in p_stack[:5]])
if b_stack:
    print('First unclosed { at lines:', [x[0] for x in b_stack[:5]])
if k_stack:
    print('First unclosed [ at lines:', [x[0] for x in k_stack[:5]])
