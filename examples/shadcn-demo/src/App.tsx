import { useEffect, useState } from "react"
import { Menu, Window, app, dialog } from "gpuijs/react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"

export function App({ setDark }: { setDark: (dark: boolean) => void }) {
  const [count, setCount] = useState(0)
  const [name, setName] = useState("")
  const [saved, setSaved] = useState<string | null>(null)
  const [dark, setDarkState] = useState(false)

  const [file, setFile] = useState<string | null>(null)

  const toggleDark = () => {
    setDarkState(!dark)
    setDark(!dark)
  }
  const openFile = async () => {
    const result = await dialog.showOpenDialog(Window.getFocusedWindow(), { properties: ["openFile"] })
    if (!result.canceled) setFile(result.filePaths[0])
  }
  const confirmReset = async () => {
    const { response } = await dialog.showMessageBox({
      type: "question",
      message: "Reset the counter?",
      detail: `It is at ${count}.`,
      buttons: ["Reset", "Cancel"],
    })
    if (response === 0) setCount(0)
  }

  // The menu bar and the dock menu, rebuilt when what they show changes.
  useEffect(() => {
    Menu.setApplicationMenu(
      Menu.buildFromTemplate([
        { role: "appMenu" },
        {
          label: "File",
          submenu: [
            { label: "Open…", accelerator: "CmdOrCtrl+O", click: openFile },
            { type: "separator" },
            { label: "Export", enabled: false },
          ],
        },
        { role: "editMenu" },
        {
          label: "View",
          submenu: [
            { label: "Dark Mode", type: "checkbox", checked: dark, accelerator: "CmdOrCtrl+Shift+D", click: toggleDark },
            { type: "separator" },
            { role: "togglefullscreen" },
          ],
        },
        { role: "windowMenu" },
      ]),
    )
    app.dock.setMenu(Menu.buildFromTemplate([{ label: "Count Up", click: () => setCount((c) => c + 1) }]))
  }, [dark])

  const contextMenu = () =>
    Menu.buildFromTemplate([
      { label: "Count Up", click: () => setCount((c) => c + 1) },
      { label: "Reset…", click: confirmReset },
      { type: "separator" },
      { label: "Dark Mode", type: "checkbox", checked: dark, click: toggleDark },
    ]).popup()

  return (
    <div className="flex min-h-screen flex-col">
      <header className="bg-background/80 sticky top-0 flex h-[52px] shrink-0 items-center justify-center border-b text-sm font-medium [app-region:drag]">
        gpuijs demo
        <Button className="absolute right-3 [app-region:no-drag]" size="sm" variant="ghost" onClick={openFile}>
          Open…
        </Button>
      </header>
      <div className="flex flex-col items-center gap-6 p-8">
        <Card className="w-full max-w-md" onContextMenu={contextMenu}>
          <CardHeader>
            <CardTitle>Account settings</CardTitle>
            <CardDescription>Rendered by gpui, styled by Tailwind and shadcn/ui.</CardDescription>
            <CardAction>
              <Badge variant="secondary">Beta</Badge>
            </CardAction>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="name">Display name</Label>
              <Input
                id="name"
                placeholder="Ada Lovelace"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && setSaved(name)}
              />
            </div>
            <p className="text-muted-foreground text-sm">
              Hello, <strong className="text-foreground">{name || "stranger"}</strong>. Press Enter to save.
            </p>
            {saved !== null && <Badge variant="outline">Saved “{saved}”</Badge>}
            {file && <Badge variant="secondary">Opened {file}</Badge>}
          </CardContent>
          <Separator />
          <CardFooter className="flex-wrap gap-2">
            <Button onClick={() => setCount(count + 1)}>Clicked {count} times</Button>
            <Button variant="outline" onClick={confirmReset}>
              Reset
            </Button>
            <Button variant="secondary" onClick={toggleDark}>
              {dark ? "Light" : "Dark"} mode
            </Button>
            <Button variant="destructive">Delete</Button>
            <Button variant="ghost">Ghost</Button>
          </CardFooter>
        </Card>

        <div className="custom-panel">
          <h2>Plain CSS</h2>
          <p>This panel has no Tailwind classes: nesting, a custom property, a gradient and :hover.</p>
        </div>

        <div>
          <Badge>Default</Badge> <Badge variant="secondary">Secondary</Badge>{" "}
          <Badge variant="destructive">Destructive</Badge> <Badge variant="outline">Outline</Badge>
        </div>
      </div>
    </div>
  )
}
