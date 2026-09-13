# WEB-037 review cycle

**Status:** Delivered
**Branch:** `web/web-037-jabber-messages`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | none |

## The body renderer

Tokenised, not escaped-then-regexed. Escaping first turns an `&` inside a URL into `&amp;` and the
href stops working; linkifying first means the surrounding text never gets escaped at all. So the
text is split into URL and non-URL runs, each escaped on its own terms, and mentions are marked only
inside the runs that are not markup.

Trailing sentence punctuation is trimmed off a URL, the same set `trim_url_tail` strips, because a
copied link with a full stop on the end fails when it is pasted.

Checked against the four cases that matter:

```
condense("Bot requests the attention of: a, b, c, d")
  -> "Bot requests the attention of: [4 users]"
bodyHtml("see https://zkillboard.com/kill/1?a=1&b=2 now, Smense", ["smense"])
  -> ...<a href="https://zkillboard.com/kill/1?a=1&amp;b=2">...</a> now, <b class="jmention">Smense</b>
bodyHtml("<script>x</script> smense!", ["smense"])
  -> &lt;script&gt;x&lt;/script&gt; <b class="jmention">smense</b>!
```

The mention keeps the casing it was written in, and the script tag stays text.

## Mentions

The names come from the app's own `mention_names`, sent rather than re-derived, so the highlight and
the unread mention marker cannot disagree about what counts as being named.

## Grouping

Same sender, within five minutes, and nobody else in between, which the "nobody else" part gets for
free from comparing against the previous message rather than the previous message *from that sender*.
A block drops the name and the time and keeps the body, which is what makes it a block rather than a
merge.
