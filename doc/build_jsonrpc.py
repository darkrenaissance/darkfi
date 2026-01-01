#!/usr/bin/env python3
import json
from sys import argv, stderr


def main(path):
    print(f"Opening {path}", file=stderr)
    with open(path, "r") as f:
        lines = [line.strip() for line in f.readlines()]
    
    methods = []
    parsing = False
    parsing_request = False
    parsing_response = False
    
    method = ""
    comment = ""
    request_lines = []
    response_lines = []
    start_line = 0
    
    for idx, line in enumerate(lines):
        if not line.startswith("//"):
            if parsing_response and response_lines:
                request_json = " ".join(request_lines)
                response_json = " ".join(response_lines)
                
                parsed_req = json.loads(request_json)
                parsed_res = json.loads(response_json)
                
                methods.append({
                    "method": parsed_req["method"],
                    "comment": comment.strip(),
                    "request": json.dumps(parsed_req, indent=2),
                    "response": json.dumps(parsed_res, indent=2),
                    "line": start_line
                })
                
                parsing = False
                parsing_request = False
                parsing_response = False
                request_lines = []
                response_lines = []
            continue
        
        text = line[2:].strip()
        
        if text == "RPCAPI:":
            if parsing_response and response_lines:
                request_json = " ".join(request_lines)
                response_json = " ".join(response_lines)
                
                parsed_req = json.loads(request_json)
                parsed_res = json.loads(response_json)
                
                methods.append({
                    "method": parsed_req["method"],
                    "comment": comment.strip(),
                    "request": json.dumps(parsed_req, indent=2),
                    "response": json.dumps(parsed_res, indent=2),
                    "line": start_line
                })
                
                parsing_request = False
                parsing_response = False
                request_lines = []
                response_lines = []
            
            parsing = True
            start_line = idx + 1
            comment = ""
            continue
        
        if not parsing:
            continue
        
        if text.startswith("-->"):
            parsing_request = True
            request_lines = [text[3:].strip()]
            continue
        
        if text.startswith("<--"):
            parsing_request = False
            parsing_response = True
            response_lines = [text[3:].strip()]
            continue
        
        if parsing_response:
            response_lines.append(text)
        elif parsing_request:
            request_lines.append(text)
        else:
            comment += text + "\n"
    
    if parsing_response and response_lines:
        request_json = " ".join(request_lines)
        response_json = " ".join(response_lines)
        
        parsed_req = json.loads(request_json)
        parsed_res = json.loads(response_json)
        
        methods.append({
            "method": parsed_req["method"],
            "comment": comment.strip(),
            "request": json.dumps(parsed_req, indent=2),
            "response": json.dumps(parsed_res, indent=2),
            "line": start_line
        })

    for m in methods:
        anchor = m["method"].replace(".", "").replace("/", "").lower()
        print(f"### `{m['method']}`\n")
        ghlink = f"https://codeberg.org/darkrenaissance/darkfi/src/branch/master/{path.replace('../', '')}#L{m['line']}"
        print(f'<sup><a href="{ghlink}">[source]</a></sup>\n')

        if m["comment"]:
            print(f"{m['comment']}\n")
        
        print("**Request:**\n")
        print("```json")
        print(m["request"])
        print("```\n")
        
        print("**Response:**\n")
        print("```json")
        print(m["response"])
        print("```\n")


if __name__ == "__main__":
    main(argv[1])
