---
tags: [moc, architecture]
---

# 20 - Architecture Overview

How fuxx-cli works, at a high level. This is the map for the architecture layer.

## The whole thing in one diagram

```
   you type:  fuxx claude
                  |
                  v
   +------------------------------+
   |          fuxx-cli            |
   |  1. parse args               |
   |  2. spawn `claude` as child  | <----- [[21 - The Wrapper Process Model]]
   |  3. read child's output      | <----- [[22 - The Output Stream Pipeline]]
   |  4. scan for OSC 9/99/777     | <----- [[25 - OSC Sequence Detection]]
   |  5. on match -> notify()     | <----- [[26 - Notification Dispatch]]
   +------------------------------+
                  |
                  v
        native macOS notification
```

## The four architectural pieces

- [[21 - The Wrapper Process Model]] — how fuxx-cli launches and hosts the agent
- [[22 - The Output Stream Pipeline]] — how output flows through fuxx-cli
- [[25 - OSC Sequence Detection]] — recognizing the "needs attention" signal
- [[26 - Notification Dispatch]] — turning a detection into a desktop alert

## Key decisions

- [[23 - Why Not libghostty]] — why there is no terminal emulation here
- [[24 - Future - Daemon and Dashboard]] — where the architecture could grow later

## Related

- [[30 - Tech Stack Overview]]
- [[40 - Development Roadmap]]
