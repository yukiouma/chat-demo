# CRF Agent Demo

A desktop chat client for a local LLM, backed by an HTTP service that owns user identity, conversation history, and the catalogue of available models.

## Language

### Identity

**User**:
The human who operates the desktop app. The system ships with a single seeded account.
_Avoid_: Account, Customer, Profile

**Credential**:
The username and password pair a User presents to authenticate.
_Avoid_: Auth session, Token

**Assistant**:
The non-human participant in a Conversation, which produces Messages in response to the User's. Backed by a Model, but distinct from it.
_Avoid_: Bot, AI, LLM, Model

**Application**:
The desktop app the User operates. It authors a Conversation's Instructions, and writes the outcome of each Tool Invocation.
_Avoid_: Assistant, System, Backend

### Conversations

**Conversation**:
A single chat thread belonging to one User — the unit that is created, switched between, forked, and deleted.
_Avoid_: Session, Chat, Thread

**Message**:
One entry in a Conversation, made of an ordered list of Message Parts. Its role names the participant it belongs to, but the Parts within it are not all written by that participant. The assistant's Message covers a whole exchange: when the model calls a tool, the call, its result and the answer that followed are all Parts of one Message.
_Avoid_: Chat message, Turn (see below)

**Instructions**:
A Message, authored by the Application, holding the Conversation's standing configuration for the model. It is not speech, and it is never shown in the transcript. The User may set it, and may leave it out entirely.
_Avoid_: System Prompt, Prompt, Preamble, Directive

**Message Part**:
A typed fragment of a Message. The kinds are Text Part, Reasoning Part, and Tool Invocation.
_Avoid_: Content, Chunk, Segment

**Text Part**:
A Message Part holding the assistant's prose answer.
_Avoid_: Response, Completion, Output

**Reasoning Part**:
A Message Part holding the model's chain-of-thought, shown separately from the answer it produced.
_Avoid_: Thinking, Chain of thought, Scratchpad

**Tool Invocation**:
A Message Part recording the model's request to run a named tool and the arguments it chose, together with the outcome. The model writes the request; the Application writes the outcome.
_Avoid_: Tool call, Function call, Command

**Turn**:
One User Message together with the assistant Message that answers it.
_Avoid_: Exchange, Request

**Fork**:
The act of creating a new Conversation that carries an existing Conversation's Messages up to a chosen point, then continues independently.
_Avoid_: Branch, Clone, Copy, Duplicate

**Lineage**:
The recorded relationship between a forked Conversation and the Conversation it was forked from.
_Avoid_: Parent, Origin, Source

### Models

**Model**:
An LLM the service offers for use, identified by the name the inference engine knows it by.
_Avoid_: Engine, Provider, LLM
