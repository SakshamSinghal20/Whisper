#!/bin/bash

set -e

echo "🔮 Whisper Silent Payments Indexer - Quick Start"
echo "================================================"
echo ""

# Check prerequisites
check_command() {
    if ! command -v $1 &> /dev/null; then
        echo "❌ $1 is not installed. Please install it first."
        exit 1
    fi
}

echo "Checking prerequisites..."
check_command cargo
check_command psql
check_command docker
check_command docker-compose

echo "✓ All prerequisites found"
echo ""

# Setup environment
if [ ! -f .env ]; then
    echo "Creating .env file..."
    cp .env.example .env
    echo "✓ .env created (please review and update if needed)"
else
    echo "✓ .env already exists"
fi

echo ""
echo "Choose setup method:"
echo "1) Docker Compose (recommended)"
echo "2) Manual setup"
read -p "Enter choice [1-2]: " choice

case $choice in
    1)
        echo ""
        echo "Starting Docker services..."
        docker-compose up -d
        
        echo ""
        echo "Waiting for services to be ready..."
        sleep 10
        
        echo ""
        echo "Checking server status..."
        curl -s http://localhost:3000/api/v1/status | jq . || echo "Server starting..."
        
        echo ""
        echo "✓ Setup complete!"
        echo ""
        echo "Services running:"
        echo "  - PostgreSQL: localhost:5432"
        echo "  - Bitcoin Core (regtest): localhost:18443"
        echo "  - Whisper API: http://localhost:3000"
        echo ""
        echo "Useful commands:"
        echo "  docker-compose logs -f whisper-server  # View logs"
        echo "  docker-compose down                     # Stop services"
        echo "  make example                            # Run client example"
        ;;
    2)
        echo ""
        echo "Manual setup selected."
        echo ""
        
        # Check PostgreSQL
        read -p "PostgreSQL database name [whisper]: " dbname
        dbname=${dbname:-whisper}
        
        if psql -lqt | cut -d \| -f 1 | grep -qw $dbname; then
            echo "✓ Database '$dbname' exists"
        else
            echo "Creating database '$dbname'..."
            createdb $dbname
            echo "✓ Database created"
        fi
        
        # Build
        echo ""
        echo "Building Whisper..."
        cargo build --release
        
        echo ""
        echo "Running tests..."
        cargo test --all
        
        echo ""
        echo "✓ Build complete!"
        echo ""
        echo "Next steps:"
        echo "1. Configure Bitcoin Core (see SETUP.md)"
        echo "2. Update .env with your settings"
        echo "3. Run: cd whisper-server && cargo run --release"
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

echo ""
echo "📚 Documentation:"
echo "  - README.md  - Project overview"
echo "  - SETUP.md   - Detailed setup guide"
echo ""
echo "🎉 Happy indexing!"
