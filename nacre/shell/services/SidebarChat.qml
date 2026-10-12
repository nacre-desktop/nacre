pragma Singleton
import Quickshell.Io

NacreChatSession {
    id: root
    IpcHandler {
        target: "sidebarChat"
        function state(): string {
            return JSON.stringify({
                provider: NacreAiViews.edge.provider,
                title: NacreAiViews.edge.title,
                threadId: NacreAiViews.edge.threadId,
                busy: NacreAiViews.edge.busy,
                inWorkspace: NacreAiViews.edge.inWorkspace,
                connected: NacreAiViews.edge.connected,
                messages: NacreAiViews.edge.messages.count,
                error: NacreAiViews.edge.error,
                draftLength: NacreAiViews.edge.draft.length,
                attachments: NacreAiViews.edge.attachments.length,
                attachmentSupport: NacreAiViews.edge.attachmentsSupported,
                composerBusy: NacreAiViews.edge.composerBusy
            });
        }
        function send(text: string): void {
            NacreAiViews.edge.send(text);
        }
        function stop(): void {
            NacreAiViews.edge.stop();
        }
    }
}
