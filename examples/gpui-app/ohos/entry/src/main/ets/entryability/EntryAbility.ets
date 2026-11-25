import { LazyPlugin, NativeAbility } from "@ohos-rs/ability";
import { AppControlPlugin } from "@ohos-rs/ability-plugin-app-control";
import { ClipboardPlugin } from "@ohos-rs/ability-plugin-clipboard";
import { FilesPlugin } from "@ohos-rs/ability-plugin-files";
import { MenuPlugin } from "@ohos-rs/ability-plugin-menu";
import { NotificationPlugin } from "@ohos-rs/ability-plugin-notification";
import { PermissionPlugin } from "@ohos-rs/ability-plugin-permission";
import { SystemStatePlugin } from "@ohos-rs/ability-plugin-system-state";
import { UrlPlugin } from "@ohos-rs/ability-plugin-url";
import { WindowPlugin } from "@ohos-rs/ability-plugin-window";

export default class EntryAbility extends NativeAbility {
  public moduleName: string = "monio_gpui";
  public defaultPage: boolean = true;
  public bridgePlugins = [
    new LazyPlugin(() => new AppControlPlugin()),
    new LazyPlugin(() => new ClipboardPlugin()),
    new LazyPlugin(() => new FilesPlugin()),
    new LazyPlugin(() => new MenuPlugin()),
    new LazyPlugin(() => new NotificationPlugin({ scheme: "monio-gpui-notification" })),
    new LazyPlugin(() => new PermissionPlugin()),
    new LazyPlugin(() => new SystemStatePlugin()),
    new LazyPlugin(() => new UrlPlugin()),
    new LazyPlugin(() => new WindowPlugin()),
  ];
}
