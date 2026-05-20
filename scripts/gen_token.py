#!/usr/bin/env python3
"""Generate dev JWT for JobFirst -> upload-service."""
import os
import sys
import datetime

try:
    import jwt
except ImportError:
    print("Run: pip install pyjwt", file=sys.stderr)
    sys.exit(1)

SECRET = os.environ.get("JWT_SECRET", "adirp-secret-key-2024")
USER_ID = os.environ.get("USER_ID", "1")

payload = {
    "sub": USER_ID,
    "role": "candidate",
    "exp": datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(hours=24),
    "iat": datetime.datetime.now(datetime.timezone.utc),
    "name": "dev_user",
    "phone": "13800138000",
}

token = jwt.encode(payload, SECRET, algorithm="HS256")
if isinstance(token, bytes):
    token = token.decode()
print(token)
