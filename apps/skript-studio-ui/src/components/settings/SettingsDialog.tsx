/**
 * Settings dialog. Two tabs: Appearance + LSP.
 */

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Palette, Server } from "lucide-react";
import { AppearanceSettings } from "./AppearanceSettings";
import { LSPSettings } from "./LSPSettings";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure editor appearance and language server behaviour.
            Changes persist to disk automatically.
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="appearance" className="mt-2">
          <TabsList>
            <TabsTrigger value="appearance" className="gap-1.5">
              <Palette className="h-3.5 w-3.5" />
              Appearance
            </TabsTrigger>
            <TabsTrigger value="lsp" className="gap-1.5">
              <Server className="h-3.5 w-3.5" />
              Language Server
            </TabsTrigger>
          </TabsList>
          <TabsContent value="appearance">
            <AppearanceSettings />
          </TabsContent>
          <TabsContent value="lsp">
            <LSPSettings />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
