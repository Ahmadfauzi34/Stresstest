import re
import os
import shutil
import sys

def patch_javascript_code(file_path, target_function, new_body, create_backup=True):
    """
    Upgraded skill to patch JavaScript functions in .html or .js files.
    Supports: traditional functions, arrow functions, async functions, and class methods.
    """
    if not os.path.exists(file_path):
        return f"Error: File {file_path} not found."

    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
    except Exception as e:
        return f"Error reading file: {e}"

    # Patterns to match different function styles
    patterns = [
        # traditional: function name() { ... } or async function name() { ... }
        rf"(?P<prefix>(?:async\s+)?function\s+{target_function}\s*\([^)]*\)\s*)\{{",
        # arrow/expression: const name = (...) => { ... }
        rf"(?P<prefix>(?:const|let|var)\s+{target_function}\s*=\s*(?:async\s*)?(?:\([^)]*\)|[\w$]+)\s*=>\s*)\{{",
        # class method: name() { ... } or static name() { ... } or async name() { ... }
        rf"(?P<prefix>(?:(?:static|async)\s+)*{target_function}\s*\([^)]*\)\s*)\{{"
    ]

    match = None
    for pattern in patterns:
        match = re.search(pattern, content)
        if match:
            break

    if not match:
        return f"Error: Function '{target_function}' not found in file."

    start_index = match.end()

    # Bracket matching with string and comment awareness
    bracket_count = 1
    end_index = -1
    in_string = None # ", ', or `
    in_comment = None # // or /*

    i = start_index
    while i < len(content):
        char = content[i]

        # Handle comments
        if not in_string:
            if not in_comment:
                if content[i:i+2] == "//":
                    in_comment = "//"
                    i += 2
                    continue
                elif content[i:i+2] == "/*":
                    in_comment = "/*"
                    i += 2
                    continue
            elif in_comment == "//" and char == "\n":
                in_comment = None
            elif in_comment == "/*" and content[i:i+2] == "*/":
                in_comment = None
                i += 2
                continue

        if in_comment:
            i += 1
            continue

        # Handle strings
        if not in_string:
            if char in ("'", '"', '`'):
                in_string = char
        elif char == in_string:
            # Check for escape: count backslashes before the quote
            backslash_count = 0
            j = i - 1
            while j >= 0 and content[j] == "\\":
                backslash_count += 1
                j -= 1
            if backslash_count % 2 == 0: # Even number of backslashes means the quote is NOT escaped
                in_string = None

        if in_string:
            i += 1
            continue

        # Match brackets
        if char == '{':
            bracket_count += 1
        elif char == '}':
            bracket_count -= 1

        if bracket_count == 0:
            end_index = i
            break
        i += 1

    if end_index == -1:
        return f"Error: Failed to find closing brace for function {target_function}."

    # Backup
    if create_backup:
        shutil.copy2(file_path, file_path + ".bak")

    # Detect indentation of the line where the function starts
    line_start = content.rfind('\n', 0, match.start()) + 1
    indentation = ""
    for c in content[line_start:match.start()]:
        if c.isspace():
            indentation += c
        else:
            break

    # Prepare the new body with proper indentation
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

    return f"Success: Function '{target_function}' in '{file_path}' patched successfully."

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: python js_patcher.py <file_path> <target_function> <new_body_file>")
        sys.exit(1)

    path = sys.argv[1]
    func = sys.argv[2]
    body_file = sys.argv[3]

    if not os.path.exists(body_file):
        print(f"Error: Body file {body_file} not found.")
        sys.exit(1)

    with open(body_file, 'r') as f:
        body = f.read()

    print(patch_javascript_code(path, func, body))
