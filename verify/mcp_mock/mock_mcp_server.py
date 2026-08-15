#!/usr/bin/env python3

import json
import sys


def send(payload):
    print(json.dumps(payload), flush=True)


for line in sys.stdin:
    request = json.loads(line)

    method = request.get("method")

    if method == "tools/list":
        send({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Return input text",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "text": {
                                    "type": "string"
                                }
                            }
                        }
                    }
                ]
            }
        })

    elif method == "tools/call":
        arguments = request.get("params", {}).get("arguments", {})

        send({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": arguments.get("text", "")
                    }
                ]
            }
        })

    else:
        send({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "error": {
                "message": "unknown method"
            }
        })
