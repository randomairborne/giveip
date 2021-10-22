#!/bin/bash
cd /var/www/ip.mcfix.org
./venv/bin/python -m gunicorn app:app -b 0.0.0.0:3333
