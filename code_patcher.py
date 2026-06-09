import re
import os
import shutil
import sys
import json

def patch_code(file_path, target, new_body, create_backup=True, dry_run=False):
    """
    Advanced code patching tool for JS, TS, CSS, HTML.
    Returns: dict {status, message, target, file_path, pattern_type, backup_created, original_content, patched_content}
    """
    result = {
        "status": "error",
        "message": "",
        "file_path": file_path,
        "target": target,
        "pattern_type": None,
        "backup_created": False,
        "original_content": None,
        "patched_content": None
    }

    if not os.path.exists(file_path):
        result["message"] = f"File {file_path} not found."
        return result

    ext = os.path.splitext(file_path)[1].lower()

    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
            result["original_content"] = content
    except Exception as e:
        result["message"] = f"Error reading file: {e}"
        return result

    # 5 Patterns for JS/TS/HTML Script + 1 for CSS
    patterns = {
        "traditional": rf"(?P<prefix>(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
        "arrow": rf"(?P<prefix>(?:export\s+)?(?:const|let|var)\s+{target}\s*(?::\s*[^=]+)?\s*=\s*(?:async\s*)?(?:<[^>]+>)?\s*(?:\([^)]*\)|[\w$]+)(?:\s*:\s*[^=]+)?\s*=>\s*)\{{",
        "method": rf"(?P<prefix>(?:@[\w.]+(?:\([^)]*\))?\s+)*(?:(?:static|async|public|private|protected|get|set)\s+)*{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
        "property": rf"(?P<prefix>{target}\s*:\s*(?:async\s*)?(?:function\s*(?:<[^>]+>)?\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{",
        "assignment": rf"(?P<prefix>(?:exports|module\.exports|window|globalThis|this)\.{target}\s*=\s*(?:async\s*)?(?:function\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{"
    }

    if ext == '.css':
        patterns = {"css_rule": rf"(?P<prefix>(?:@[\w-]+\s+)?{re.escape(target)}\s*)\{{"}
    elif ext not in ('.js', '.ts', '.tsx', '.jsx', '.mjs', '.cjs', '.html'):
        result["message"] = f"Unsupported extension {ext}"
        return result

    match = None
    matched_type = None
    for p_type, p_regex in patterns.items():
        match = re.search(p_regex, content, re.DOTALL)
        if match:
            matched_type = p_type
            break

    if not match:
        result["message"] = f"Target '{target}' not found."
        return result

    result["pattern_type"] = matched_type
    start_index = match.end()

    bracket_count = 1
    end_index = -1
    in_string = None; in_comment = None; in_regex = False

    i = start_index
    while i < len(content):
        char = content[i]
        next_char = content[i+1] if i+1 < len(content) else ""
        prev_char = content[i-1] if i > 0 else ""

        # 1. Comments
        if not in_string and not in_regex:
            if not in_comment:
                if char == "/" and next_char == "/": in_comment = "//"; i += 2; continue
                elif char == "/" and next_char == "*": in_comment = "/*"; i += 2; continue
            elif in_comment == "//" and char == "\n": in_comment = None
            elif in_comment == "/*" and char == "*" and next_char == "/": in_comment = None; i += 2; continue
        if in_comment: i += 1; continue

        # 2. Regex Literals
        if ext != '.css' and not in_string and not in_comment:
            if not in_regex:
                if char == "/":
                    lookback = content[max(0, i-20):i].strip()
                    if lookback and (lookback[-1] in "(=:[!&|?~,;" or lookback.endswith("return")):
                        in_regex = True; i += 1; continue
            elif in_regex:
                if char == "/" and prev_char != "\\": in_regex = False; i += 1; continue
        if in_regex: i += 1; continue

        # 3. Strings
        if not in_string:
            if char in ("'", '"', '`'): in_string = char
        elif char == in_string:
            bs_count = 0; j = i - 1
            while j >= 0 and content[j] == "\\": bs_count += 1; j -= 1
            if bs_count % 2 == 0: in_string = None
        if in_string: i += 1; continue

        # 4. Brackets
        if char == '{': bracket_count += 1
        elif char == '}': bracket_count -= 1
        if bracket_count == 0: end_index = i; break
        i += 1

    if end_index == -1:
        result["message"] = f"Closing brace not found for {target}."
        return result

    line_start = content.rfind('\n', 0, match.start()) + 1
    indentation = ""
    for c in content[line_start:match.start()]:
        if c.isspace(): indentation += c
        else: break

    indented_body = ""
    body_lines = new_body.strip('\n').split('\n')
    extra_indent = "    " if "\t" not in indentation else "\t"
    for line in body_lines:
        if line.strip(): indented_body += indentation + extra_indent + line.strip() + "\n"
        else: indented_body += "\n"

    patched_content = content[:start_index] + "\n" + indented_body + indentation + content[end_index:]
    result["patched_content"] = patched_content

    if not dry_run:
        if create_backup:
            try:
                shutil.copy2(file_path, file_path + ".bak")
                result["backup_created"] = True
            except Exception as e:
                result["message"] = f"Backup failed: {e}"
                return result

        try:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(patched_content)
            result["status"] = "success"
            result["message"] = f"Successfully patched '{target}' in {file_path}."
        except Exception as e:
            result["message"] = f"Write failed: {e}"
    else:
        result["status"] = "success"
        result["message"] = f"Dry-run: '{target}' patch preview generated."

    return result

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Advanced Code Patcher")
    parser.add_argument("file_path", help="Path to the file")
    parser.add_argument("target", help="Function name or CSS selector")
    parser.add_argument("body_file", help="File containing the new body")
    parser.add_argument("--no-backup", action="store_false", dest="backup", help="Disable backup creation")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without writing")

    args = parser.parse_args()

    with open(args.body_file, 'r') as f:
        body = f.read()

    res = patch_code(args.file_path, args.target, body, create_backup=args.backup, dry_run=args.dry_run)
    print(json.dumps(res, indent=2))
