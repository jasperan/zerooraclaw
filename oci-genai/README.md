# OCI Generative AI Integration for ZeroOraClaw

Optional LLM backend using [OCI Generative AI](https://docs.oracle.com/en-us/iaas/Content/generative-ai/home.htm) service. This module provides an OpenAI-compatible local proxy that forwards requests to OCI GenAI using native OCI authentication.

The default LLM backend (Ollama) remains unchanged. OCI GenAI is an **optional alternative** for users who want to leverage enterprise models available through Oracle Cloud Infrastructure.

## Prerequisites

- **Python 3.11+**
- **OCI CLI configured** with a valid `~/.oci/config` profile
- **OCI Compartment** with Generative AI service enabled

## Quick Start

### 1. Install dependencies

```bash
cd oci-genai
pip install -r requirements.txt
```

### 2. Set environment variables

```bash
export OCI_PROFILE=DEFAULT
export OCI_REGION=us-chicago-1
export OCI_COMPARTMENT_ID=ocid1.compartment.oc1..your-compartment-ocid
```

### 3. Start the proxy

```bash
python proxy.py
# OCI GenAI proxy listening on http://localhost:9999/v1
```

### 4. Configure ZeroOraClaw

Set ZeroOraClaw to use the OpenAI provider pointing at the local proxy:

```toml
# ~/.zerooraclaw/config.toml
[provider]
name = "openai"
api_base = "http://localhost:9999/v1"
api_key = "oci-genai"
model = "meta.llama-3.3-70b-instruct"
```

Or via environment variables:

```bash
PROVIDER=openai API_KEY=oci-genai ./zerooraclaw agent -m "Hello"
```

## Environment Variables

| Variable | Description | Default |
|---|---|---|
| `OCI_PROFILE` | OCI config profile name | `DEFAULT` |
| `OCI_REGION` | OCI region for GenAI endpoint | `us-chicago-1` |
| `OCI_COMPARTMENT_ID` | OCI compartment OCID (**required**) | -- |
| `OCI_PROXY_PORT` | Local proxy listen port | `9999` |
| `OCI_GENAI_MODEL` | Model identifier for GenAI | `meta.llama-3.3-70b-instruct` |

## Available OCI GenAI Models

OCI Generative AI provides access to several model families:

- **Meta Llama** -- `meta.llama-3.3-70b-instruct`, `meta.llama-3.1-405b-instruct`
- **Cohere** -- `cohere.command-r-plus`, `cohere.command-r`
- **xAI Grok** -- available in select regions

Model availability varies by region. Check the [OCI GenAI documentation](https://docs.oracle.com/en-us/iaas/Content/generative-ai/pretrained-models.htm) for current model listings.

## Architecture

```
ZeroOraClaw (Rust)
    |
    | HTTP (OpenAI-compatible)
    v
oci-genai/proxy.py (Python)
    |
    | OCI SDK Auth (User Principal)
    v
OCI Generative AI Service
```

The proxy translates standard OpenAI API calls into OCI-authenticated requests using the `oci-openai` library. This means ZeroOraClaw treats it as any other OpenAI-compatible provider -- no Rust code changes needed.

## Files

| File | Description |
|---|---|
| `oci_client.py` | OCI GenAI client wrapper (sync + async) |
| `proxy.py` | Local OpenAI-compatible proxy server |
| `requirements.txt` | Python dependencies |

## Further Reading

- [OCI Generative AI Documentation](https://docs.oracle.com/en-us/iaas/Content/generative-ai/home.htm)
- [OCI Generative AI Pretrained Models](https://docs.oracle.com/en-us/iaas/Content/generative-ai/pretrained-models.htm)
- [oci-openai Python Package](https://pypi.org/project/oci-openai/)
- [OCI CLI Configuration](https://docs.oracle.com/en-us/iaas/Content/API/Concepts/sdkconfig.htm)
