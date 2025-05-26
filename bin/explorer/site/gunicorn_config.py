## explorer gunicorn configuration

# Bind address and port
bind = "127.0.0.1:8000"

# Number of worker processes
workers = 1

# Number of threads per worker
threads = 2

# Type of worker class
worker_class = "gthread"

# Maximum number of pending connections
backlog = 2048

# Timeout for workers (in seconds)
timeout = 30

# PID file location
pidfile = "gunicorn.pid"

# Log configuration
import os
environment = os.getenv("FLASK_ENV", "testnet")
log_home = os.getenv("LOG_HOME", "/tmp")
log_path = os.path.join(log_home, environment, "app.log")
accesslog = log_path
errorlog = log_path
loglevel = "info"
