# mem0 Memory Integration

Production-ready memory layer for AG-Claw with +26% accuracy vs baseline.

## Setup

1. Get API key from https://app.mem0.ai
2. Set environment variable:
   ```bash
   export MEM0_API_KEY=your_key_here
   ```

## Configuration

In config/default.yaml:
```yaml
features:
  mem0-memory:
    enabled: false
    userId: "ag-claw-user"
    model: "gpt-4"
```

## Usage

mem0 provides adaptive memory that learns user preferences over time.
