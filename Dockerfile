FROM python:3.12-slim
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 HOST=0.0.0.0
WORKDIR /app
COPY pyproject.toml requirements.lock README.md LICENSE ./
COPY whisper ./whisper
RUN pip install --no-cache-dir -c requirements.lock . \
    && useradd --create-home --uid 10001 whisper \
    && mkdir /data && chown whisper:whisper /data
USER whisper
ENV DATABASE_URL=sqlite+aiosqlite:////data/whisper.db
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:3000/api/v1/blocks?limit=1', timeout=4)"
CMD ["python", "-m", "whisper"]
