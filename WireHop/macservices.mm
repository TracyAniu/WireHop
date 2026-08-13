// SPDX-License-Identifier: BSD-3-Clause

#import <AppKit/AppKit.h>

#include <QMetaObject>
#include <QStringList>

#include "macservices.h"
#include "trayicon.h"

@interface WireHopServiceProvider : NSObject {
@public
    TrayIcon *trayIcon;
}
- (void)sendWithWireHop:(NSPasteboard *)pboard userData:(NSString *)userData error:(NSString **)error;
@end

@implementation WireHopServiceProvider

- (void)sendWithWireHop:(NSPasteboard *)pboard userData:(NSString *)userData error:(NSString **)error
{
    Q_UNUSED(userData);

    NSArray<NSURL *> *urls =
            [pboard readObjectsForClasses:@[ [NSURL class] ]
                                  options:@{NSPasteboardURLReadingFileURLsOnlyKey : @YES}];
    QStringList paths;
    for (NSURL *url in urls) {
        if (url.fileURL)
            paths.append(QString::fromNSString(url.path));
    }

    if (paths.isEmpty()) {
        if (error)
            *error = @"No files were provided.";
        return;
    }

    TrayIcon *target = trayIcon;
    if (!target)
        return;
    // Hop onto the Qt event loop before touching widgets.
    QMetaObject::invokeMethod(target, [target, paths]() {
        target->sendFiles(paths);
    }, Qt::QueuedConnection);
}

@end

void registerMacServicesProvider(TrayIcon *trayIcon)
{
    static WireHopServiceProvider *provider = nil;
    if (!provider)
        provider = [[WireHopServiceProvider alloc] init];
    provider->trayIcon = trayIcon;
    [[NSApplication sharedApplication] setServicesProvider:provider];
    NSUpdateDynamicServices();
}

void activateApplication()
{
    // activateIgnoringOtherApps: is what an accessory application needs; it is
    // deprecated on macOS 14+ in favour of NSApplication.activate, so prefer
    // that where available and keep the old call for older systems.
    NSApplication *app = [NSApplication sharedApplication];
    if (@available(macOS 14.0, *)) {
        [app activate];
    } else {
        [app activateIgnoringOtherApps:YES];
    }
}
