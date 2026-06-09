import re, os, shutil, sys, json, subprocess, difflib, tempfile
from collections import Counter

def detect_style(content):
    lines = content.splitlines()
    indents = [line[:len(line) - len(line.lstrip())] for line in lines if line.strip()]
    indents = [i for i in indents if i]
    return Counter(indents).most_common(1)[0][0] if indents else "  "

def check_bracket_balance(text, is_css=False):
    count = 0
    in_string = None
    in_comment = None
    in_regex = False
    i = 0
    while i < len(text):
        char = text[i]
        nxt = text[i+1] if i+1 < len(text) else ""
        prv = text[i-1] if i > 0 else ""

        # Comment handling
        if not in_string and not in_regex:
            if not in_comment:
                if char == "/" and nxt == "/": in_comment = "//"; i += 2; continue
                elif char == "/" and nxt == "*": in_comment = "/*"; i += 2; continue
            elif in_comment == "//" and char == "\n": in_comment = None
            elif in_comment == "/*" and char == "*" and nxt == "/": in_comment = None; i += 2; continue
        if in_comment: i += 1; continue

        # Regex literal handling (approximate)
        if not is_css and not in_string and not in_comment:
            if not in_regex:
                if char == "/":
                    lookback = text[max(0, i-20):i].strip()
                    if lookback and (lookback[-1] in "(=:[!&|?~,;" or lookback.endswith("return")):
                        in_regex = True; i += 1; continue
            elif in_regex:
                if char == "/" and prv != "\\": in_regex = False; i += 1; continue
        if in_regex: i += 1; continue

        # String handling
        if not in_string:
            if char in ("'", '"', '`'): in_string = char
        elif char == in_string:
            bs = 0
            j = i - 1
            while j >= 0 and text[j] == "\\": bs += 1; j -= 1
            if bs % 2 == 0: in_string = None
        if in_string: i += 1; continue

        # Bracket counting
        if char == '{': count += 1
        elif char == '}':
            count -= 1
            if count < 0: return False
        i += 1
    return count == 0

def validate_syntax(content, ext):
    if ext == '.html':
        scripts = re.findall(r'<script\b[^>]*>(.*?)</script>', content, re.DOTALL)
        for script in scripts:
            if not check_bracket_balance(script): return False
        return True

    if not check_bracket_balance(content, is_css=(ext=='.css')): return False

    # Optional node check for JS
    if ext in ('.js', '.cjs', '.mjs'):
        with tempfile.NamedTemporaryFile(mode='w', suffix=".js", delete=False, encoding='utf-8') as tmp:
            tmp.write(content)
            tmp_name = tmp.name
        try:
            res = subprocess.run(["node", "--check", tmp_name], capture_output=True, timeout=5)
            if res.returncode != 0:
                if b"SyntaxError" in res.stderr: return False
        except: pass
        finally:
            if os.path.exists(tmp_name): os.remove(tmp_name)
    return True

def manage_imports(content, body, ext):
    if ext not in ('.js', '.ts', '.tsx', '.jsx', '.mjs', '.cjs'): return content
    libs = {'fs': 'fs', 'path': 'path', 'os': 'os'}
    use_esm = (ext in ('.ts', '.tsx', '.mjs') or "import " in content)
    added = []
    for ident, mod in libs.items():
        if re.search(rf"\b{ident}\.", body) and not re.search(rf"\b(require|import).*{ident}", content):
            added.append(f"import {ident} from '{mod}';" if use_esm else f"const {ident} = require('{mod}');")
    return "\n".join(added) + "\n" + content if added else content

def patch_code(file_path, target, new_body, create_backup=True, dry_run=False, mode="full"):
    res = {
        "status": "error", "message": "", "file_path": file_path, "target": target,
        "pattern_type": None, "backup_created": False, "diff": "",
        "original_content": None, "patched_content": None, "validation_passed": None
    }
    if not os.path.exists(file_path): res["message"] = "File not found"; return res
    ext = os.path.splitext(file_path)[1].lower()
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
        res["original_content"] = content

    patterns = {
        "traditional": rf"(?P<prefix>(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
        "arrow": rf"(?P<prefix>(?:export\s+)?(?:const|let|var)\s+{target}\s*(?::\s*[^=]+)?\s*=\s*(?:async\s*)?(?:<[^>]+>)?\s*(?:\([^)]*\)|[\w$]+)(?:\s*:\s*[^=]+)?\s*=>\s*)\{{",
        "method": rf"(?P<prefix>(?:@[\w.]+(?:\([^)]*\))?\s+)*(?:(?:static|async|public|private|protected|get|set)\s+)*{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
        "property": rf"(?P<prefix>{target}\s*:\s*(?:async\s*)?(?:function\s*(?:<[^>]+>)?\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{",
        "assignment": rf"(?P<prefix>(?:exports|module\.exports|window|globalThis|this)\.{target}\s*=\s*(?:async\s*)?(?:function\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{"
    }
    if ext == '.css': patterns = {"css_rule": rf"(?P<prefix>(?:@[\w-]+\s+)?{re.escape(target)}\s*)\{{"}

    search_ranges = [(0, len(content))]
    if ext == '.html':
        search_ranges = [(m.start(1), m.end(1)) for m in re.finditer(r'<script\b[^>]*>(.*?)</script>', content, re.DOTALL)]

    match, mtype, s_idx, m_idx = None, None, -1, -1
    for rs, re_e in search_ranges:
        sub = content[rs:re_e]
        for pt, pr in patterns.items():
            m = re.search(pr, sub, re.DOTALL)
            if m:
                match, mtype, s_idx, m_idx = m, pt, rs + m.end(), rs + m.start()
                break
        if match: break

    if not match: res["message"] = f"Target '{target}' not found."; return res
    res["pattern_type"] = mtype

    # Body matching
    bracket_depth = 1
    end_idx = -1
    i = s_idx
    in_s, in_c, in_r = None, None, False
    while i < len(content):
        char = content[i]
        nxt = content[i+1] if i+1 < len(content) else ""
        prv = content[i-1] if i > 0 else ""
        if not in_s and not in_r:
            if not in_c:
                if char == "/" and nxt == "/": in_c = "//"; i += 2; continue
                elif char == "/" and nxt == "*": in_c = "/*"; i += 2; continue
            elif in_c == "//" and char == "\n": in_c = None
            elif in_c == "/*" and char == "*" and nxt == "/": in_c = None; i += 2; continue
        if in_c: i += 1; continue
        if ext != '.css' and not in_s and not in_c:
            if not in_r:
                if char == "/":
                    lb = content[max(0, i-20):i].strip()
                    if lb and (lb[-1] in "(=:[!&|?~,;" or lb.endswith("return")): in_r = True; i += 1; continue
            elif in_r:
                if char == "/" and prv != "\\": in_r = False; i += 1; continue
        if in_r: i += 1; continue
        if not in_s:
            if char in ("'", '"', '`'): in_s = char
        elif char == in_s:
            bs = 0; j = i - 1
            while j >= 0 and content[j] == "\\": bs += 1; j -= 1
            if bs % 2 == 0: in_s = None
        if in_s: i += 1; continue
        if char == '{': bracket_depth += 1
        elif char == '}':
            bracket_depth -= 1
            if bracket_depth == 0: end_idx = i; break
        i += 1

    if end_idx == -1: res["message"] = "Closing brace not found."; return res

    # Body preparation
    if mode == "partial" and "====" in new_body:
        search_part, replace_part = new_body.split("====", 1)
        final_body = content[s_idx:end_idx].replace(search_part.strip(), replace_part.strip())
    else:
        final_body = new_body

    style = detect_style(content)
    line_start = content.rfind('\n', 0, m_idx) + 1
    surr_indent = ""
    if line_start >= 0:
        for char in content[line_start:m_idx]:
            if char.isspace(): surr_indent += char
            else: break

    def reindent(text, base, step):
        lines = text.strip().split('\n')
        res_lines = []
        d = 1
        for l in lines:
            s = l.strip()
            if not s: res_lines.append(""); continue
            if s.startswith('}'): d -= 1
            res_lines.append(base + (step * d) + s)
            if s.endswith('{'): d += 1
        return "\n".join(res_lines)

    norm_body = reindent(final_body, surr_indent, style)

    # Safe Assembly
    head = content[:s_idx].rstrip()
    tail = content[end_idx:] # Starts with }
    patched_content = head + "\n" + norm_body + "\n" + surr_indent + tail

    patched_content = manage_imports(patched_content, final_body, ext)
    res["validation_passed"] = validate_syntax(patched_content, ext)
    if not res["validation_passed"]: res["message"] = "Syntax validation failed"; return res

    res["patched_content"] = patched_content
    res["diff"] = "".join(difflib.unified_diff(content.splitlines(keepends=True), patched_content.splitlines(keepends=True), fromfile=f"a/{os.path.basename(file_path)}", tofile=f"b/{os.path.basename(file_path)}"))

    if not dry_run:
        if create_backup:
            shutil.copy2(file_path, file_path + ".bak")
            res["backup_created"] = True
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(patched_content)
        res["status"] = "success"
        res["message"] = f"Successfully patched '{target}'."
    else:
        res["status"] = "success"
        res["message"] = "Dry-run successful."
    return res

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("file_path"); parser.add_argument("target"); parser.add_argument("body_file")
    parser.add_argument("--partial", action="store_true"); parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    with open(args.body_file, 'r') as f: body = f.read()
    print(json.dumps(patch_code(args.file_path, args.target, body, mode="partial" if args.partial else "full", dry_run=args.dry_run), indent=2))
