# Run a local model

You can run a model on your own computer, with no API key and no requests leaving
the machine. Small local models are fine for questions and small edits, and much
weaker than hosted models for larger coding tasks.

## Option 1: the local model add-on (Windows)

The Windows app can install llama.cpp and one model for you.

1. Open Settings > Providers and scroll to **Local model add-on** at the bottom.
2. Pick a model:

   | Model | Download | Suggested free memory |
   | --- | --- | --- |
   | Qwen3.5 0.8B, Q8_0 | 0.83 GB | 2 GB |
   | Qwen3.5 2B, Q4_K_M | 1.28 GB | 3 GB |
   | Qwen3.5 4B, Q4_K_M | 2.74 GB | 5 GB |

3. Press **Review**. It lists each file, its size, its host, how it is checked and
   the folder it goes to.
4. Press **Download and install**. That press approves only the files shown. Every
   file is pinned by address, size and SHA-256. **Cancel download** stops and rolls
   back.
5. Choose the local model in the model chooser. The runtime starts only when you
   choose it and approve it, and it listens only on 127.0.0.1.

**Remove add-on** stops the local model and deletes the runtime and the listed
models. A graphics card with a current Vulkan driver makes local models faster.

## Option 2: a server you already run

If you already use LM Studio, Ollama, vLLM or llama-server, pick its preset in
Settings > Providers:

| Server | Default address |
| --- | --- |
| LM Studio | `http://127.0.0.1:1234/v1/` |
| Ollama | `http://127.0.0.1:11434/v1/` |
| vLLM | `http://127.0.0.1:8000/v1/` |

For any other server, choose **OpenAI compatible** and give its address, model
name and context size.

## Use a model on your computer from your phone

1. Start the server so it listens on your network, for example:

   ```sh
   llama-server -m model.gguf --host 0.0.0.0 --port 8080 --ctx-size 32768
   ```

2. On the phone, choose **OpenAI compatible** and enter
   `http://<pc-address>:8080/v1`, the model name and the same context size.

Use plain `http://` only on a network you trust.

More detail is in
[Local model add-on](https://github.com/AG064/argentum/blob/development/docs/LOCAL_MODEL_ADDON.md).
