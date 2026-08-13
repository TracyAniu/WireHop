// SPDX-License-Identifier: BSD-3-Clause

#import <AppKit/AppKit.h>

@interface ShareViewController : NSViewController
@end

@implementation ShareViewController

- (void)loadView
{
    self.view = [[NSView alloc] initWithFrame:NSZeroRect];

    NSExtensionContext *ctx = self.extensionContext;
    NSMutableArray<NSURL *> *urls = [NSMutableArray array];
    dispatch_group_t group = dispatch_group_create();
    for (NSExtensionItem *item in ctx.inputItems) {
        for (NSItemProvider *provider in item.attachments) {
            if (![provider hasItemConformingToTypeIdentifier:@"public.file-url"])
                continue;
            dispatch_group_enter(group);
            [provider loadItemForTypeIdentifier:@"public.file-url"
                                        options:nil
                              completionHandler:^(id<NSSecureCoding> data, NSError *error) {
                NSURL *url = nil;
                if ([(NSObject *)data isKindOfClass:[NSURL class]])
                    url = (NSURL *)data;
                else if ([(NSObject *)data isKindOfClass:[NSData class]])
                    url = [NSURL URLWithDataRepresentation:(NSData *)data relativeToURL:nil];
                if (url != nil && url.fileURL) {
                    @synchronized(urls) {
                        [urls addObject:url];
                    }
                }
                dispatch_group_leave(group);
            }];
        }
    }
    dispatch_group_notify(group, dispatch_get_main_queue(), ^{
        NSURL *appURL = [[NSWorkspace sharedWorkspace]
                URLForApplicationWithBundleIdentifier:@"io.github.tracyaniu.wirehop"];
        if (urls.count == 0 || appURL == nil) {
            [ctx completeRequestReturningItems:@[] completionHandler:nil];
            return;
        }
        NSWorkspaceOpenConfiguration *config = [NSWorkspaceOpenConfiguration configuration];
        [[NSWorkspace sharedWorkspace] openURLs:urls
                           withApplicationAtURL:appURL
                                  configuration:config
                              completionHandler:^(NSRunningApplication *app, NSError *err) {
            dispatch_async(dispatch_get_main_queue(), ^{
                [ctx completeRequestReturningItems:@[] completionHandler:nil];
            });
        }];
    });
}

@end
