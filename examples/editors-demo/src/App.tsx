import { useState } from "react"
import { LexicalComposer } from "@lexical/react/LexicalComposer"
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin"
import { ContentEditable } from "@lexical/react/LexicalContentEditable"
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin"
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary"
import { OnChangePlugin } from "@lexical/react/LexicalOnChangePlugin"
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext"
import { FORMAT_TEXT_COMMAND, type EditorState } from "lexical"
import { EditorContent, useEditor, useEditorState } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"

export function App() {
  return (
    <div className="page">
      <LexicalCard />
      <TiptapCard />
      <PlainCard />
    </div>
  )
}

// Lexical's rich text setup from its getting-started guide, with a Bold button.
function LexicalCard() {
  const [text, setText] = useState("")
  const config = {
    namespace: "demo",
    theme: { text: { bold: "bold", italic: "italic" } },
    onError: (error: Error) => console.error(error),
  }
  return (
    <div className="card">
      <h2>Lexical</h2>
      <LexicalComposer initialConfig={config}>
        <LexicalToolbar />
        <RichTextPlugin
          contentEditable={<ContentEditable className="editor" aria-placeholder="Type here…" placeholder={<div className="placeholder">Type here…</div>} />}
          ErrorBoundary={LexicalErrorBoundary}
        />
        <HistoryPlugin />
        <OnChangePlugin onChange={(state: EditorState) => state.read(() => setText(JSON.stringify(state.toJSON().root.children.length) + " block(s)"))} />
      </LexicalComposer>
      <div className="log">{text}</div>
    </div>
  )
}

function LexicalToolbar() {
  const [editor] = useLexicalComposerContext()
  return (
    <div className="bar">
      <button onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "bold")}>Bold</button>
      <button onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, "italic")}>Italic</button>
    </div>
  )
}

// TipTap's StarterKit (ProseMirror), with Bold and Italic toggles.
function TiptapCard() {
  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Hello <strong>TipTap</strong>. Select some text and make it <em>italic</em>.</p>",
    editorProps: { attributes: { class: "editor" } },
  })
  const state = useEditorState({ editor, selector: ({ editor }) => ({ bold: editor.isActive("bold"), italic: editor.isActive("italic"), html: editor.getHTML() }) })
  return (
    <div className="card">
      <h2>TipTap</h2>
      <div className="bar">
        <button className={state.bold ? "on" : ""} onClick={() => editor.chain().focus().toggleBold().run()}>Bold</button>
        <button className={state.italic ? "on" : ""} onClick={() => editor.chain().focus().toggleItalic().run()}>Italic</button>
      </div>
      <EditorContent editor={editor} />
      <div className="log">{state.html}</div>
    </div>
  )
}

// A plain `contentEditable` div and its `input` events.
function PlainCard() {
  const [text, setText] = useState("")
  return (
    <div className="card">
      <h2>contentEditable</h2>
      <div className="editor" contentEditable suppressContentEditableWarning onInput={(e) => setText(e.currentTarget.textContent ?? "")}>
        <p>Edit this text.</p>
      </div>
      <div className="log">{text}</div>
    </div>
  )
}
