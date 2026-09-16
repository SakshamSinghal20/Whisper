import uvicorn

from whisper.config import Settings


def main() -> None:
    settings = Settings()
    uvicorn.run("whisper.server:app", host=settings.host, port=settings.port, log_level="info")


if __name__ == "__main__":
    main()
