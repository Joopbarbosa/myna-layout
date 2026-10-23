# myna-layout

A small status indicator for [Myna](https://github.com/canonical/myna), Canonical's offline speech-to-text dictation, on the **COSMIC** desktop (Wayland).

Myna's own HUD is a GNOME Shell extension, so on COSMIC you get no feedback while dictating. `myna-layout` watches Myna over D-Bus and shows what it is doing — loading, listening, finishing, errors — with:

- a floating bar with an animated audio-level meter, drawn with the layer-shell protocol so it never takes keyboard focus (Myna types into the focused field, so this matters);
- the system notification, as a fallback and for error reasons.

It only observes Myna. It never starts, stops or types for you.

> **Status: early development.** There is no release yet. The first target is Pop!_OS 24.04 with COSMIC. Myna itself is in preview, so its D-Bus interface may change.

**License:** planned MIT OR Apache-2.0. No code from Myna (AGPL) is copied.

---

# myna-layout (português)

Um pequeno indicador de status para o [Myna](https://github.com/canonical/myna), o ditado por voz offline da Canonical, no desktop **COSMIC** (Wayland).

O HUD oficial do Myna é uma extensão do GNOME Shell, então no COSMIC não há nenhum aviso visual durante o ditado. O `myna-layout` observa o Myna pelo D-Bus e mostra o que ele está fazendo — carregando, ouvindo, finalizando, erros — com:

- uma barra flutuante com medidor de nível de áudio animado, desenhada com o protocolo layer-shell para nunca roubar o foco do teclado (o Myna digita no campo em foco, então isso importa);
- a notificação do sistema, como alternativa e para o motivo dos erros.

Ele só observa o Myna. Nunca inicia, para nem digita por você.

> **Estado: em desenvolvimento inicial.** Ainda não há versão publicada. O primeiro alvo é o Pop!_OS 24.04 com COSMIC. O Myna está em preview, então a interface D-Bus dele pode mudar.

**Licença:** prevista MIT OU Apache-2.0. Nenhum código do Myna (AGPL) é copiado.
