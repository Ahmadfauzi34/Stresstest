import re
import os
import shutil
import sys

def patch_code(file_path, target, new_body, create_backup=True):
    """
    General code patching tool for JS, TS, CSS, and modern JS/TS variants.
    """
    if not os.path.exists(file_path):
        return f"Error: File {file_path} not found."

    ext = os.path.splitext(file_path)[1].lower()

    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
    except Exception as e:
        return f"Error reading file: {e}"

    patterns = []

    if ext in ('.js', '.ts', '.tsx', '.jsx', '.mjs', '.cjs'):
        patterns = [
            # Traditional function
            rf"(?P<prefix>(?:async\s+)?function\s+{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
            # Arrow function
            rf"(?P<prefix>(?:const|let|var)\s+{target}\s*(?::\s*[^=]+)?\s*=\s*(?:async\s*)?(?:<[^>]+>)?\s*(?:\([^)]*\)|[\w$]+)(?:\s*:\s*[^=]+)?\s*=>\s*)\{{",
            # Class method or property assignment
            rf"(?P<prefix>(?:(?:static|async|public|private|protected)\s+)*{target}\s*(?:<[^>]+>)?\s*\([^)]*\)(?:\s*:\s*[^{{]+)?\s*)\{{",
            # Property assignment: name: function() { or name: () => {
            rf"(?P<prefix>{target}\s*:\s*(?:async\s*)?(?:function\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{",
            # module.exports.name = function() {
            rf"(?P<prefix>(?:\w+\.)*{target}\s*=\s*(?:async\s*)?(?:function\s*\([^)]*\)|(?:\([^)]*\)|[\w$]+)\s*=>)\s*)\{{"
        ]
    elif ext == '.css':
        patterns = [
            rf"(?P<prefix>(?:@[\w-]+\s+)?{re.escape(target)}\s*)\{{"
        ]
    else:
        return f"Error: Unsupported file extension {ext}"

    match = None
    for pattern in patterns:
        match = re.search(pattern, content, re.DOTALL)
        if match:
            break

    if not match:
        return f"Error: '{target}' not found in file."

    start_index = match.end()

    bracket_count = 1
    end_index = -1
    in_string = None
    in_comment = None
    in_regex = False

    i = start_index
    while i < len(content):
        char = content[i]
        next_char = content[i+1] if i+1 < len(content) else ""
        prev_char = content[i-1] if i > 0 else ""

        # 1. Handle Comments
        if not in_string and not in_regex:
            if not in_comment:
                if char == "/" and next_char == "/":
                    in_comment = "//"
                    i += 2
                    continue
                elif char == "/" and next_char == "*":
                    in_comment = "/*"
                    i += 2
                    continue
            elif in_comment == "//" and char == "\n":
                in_comment = None
            elif in_comment == "/*" and char == "*" and next_char == "/":
                in_comment = None
                i += 2
                continue

        if in_comment:
            i += 1
            continue

        # 2. Handle Regex Literals (JS/TS variants)
        if ext != '.css' and not in_string and not in_comment:
            if not in_regex:
                if char == "/":
                    lookback = content[max(0, i-20):i].strip()
                    if lookback and lookback[-1] in "(=:[!&|?~,;":
                        in_regex = True
                        i += 1
                        continue
            elif in_regex:
                if char == "/" and prev_char != "\\":
                    in_regex = False
                    i += 1
                    continue

        if in_regex:
            i += 1
            continue

        # 3. Handle Strings
        if not in_string:
            if char in ("'", '"', '`'):
                in_string = char
        elif char == in_string:
            backslash_count = 0
            j = i - 1
            while j >= 0 and content[j] == "\\":
                backslash_count += 1
                j -= 1
            if backslash_count % 2 == 0:
                in_string = None

        if in_string:
            i += 1
            continue

        # 4. Bracket Matching
        if char == '{':
            bracket_count += 1
        elif char == '}':
            bracket_count -= 1

        if bracket_count == 0:
            end_index = i
            break
        i += 1

    if end_index == -1:
        return f"Error: Failed to find closing brace for {target}."

    if create_backup:
        shutil.copy2(file_path, file_path + ".bak")

    line_start = content.rfind('\n', 0, match.start()) + 1
    indentation = ""
    for c in content[line_start:match.start()]:
        if c.isspace():
            indentation += c
        else:
            break

    indented_body = ""
    body_lines = new_body.strip('\n').split('\n')
    extra_indent = "    " if "\t" not in indentation else "\t"

    for line in body_lines:
        if line.strip():
            indented_body += indentation + extra_indent + line.strip() + "\n"
        else:
            indented_body += "\n"

    new_content = content[:start_index] + "\n" + indented_body + indentation + content[end_index:]

    try:
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(new_content)
    except Exception as e:
        return f"Error writing file: {e}"

    return f"Success: '{target}' in '{file_path}' patched successfully."

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: python code_patcher.py <file_path> <target> <new_body_file>")
        sys.exit(1)

    path = sys.argv[1]
    tgt = sys.argv[2]
    body_file = sys.argv[3]

    with open(body_file, 'r') as f:
        body = f.read()

    print(patch_code(path, tgt, body))
