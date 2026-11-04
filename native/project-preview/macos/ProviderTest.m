// Compile twice with ZIPZI_THUMBNAIL=0/1, linked to the bounded Rust reader.
#import "Preview.m"
#import <objc/runtime.h>
#if !ZIPZI_THUMBNAIL
// Capture the deferred data block handed to the system and execute it as the
// host does; a non-nil reply alone cannot detect an empty preview regression.
static NSData *(^CreateData)(QLPreviewReply *, NSError **);
static IMP OriginalReplyInit;
static id CaptureReply(id object, SEL selector, UTType *type, CGSize size,
                      NSData *(^create)(QLPreviewReply *, NSError **)) {
    if (![type isEqual:UTTypePNG]) abort();
    CreateData = [create copy];
    return ((id (*)(id, SEL, UTType *, CGSize, id))OriginalReplyInit)(object, selector, type, size, create);
}
@interface PreviewRequestFixture : NSObject
@property NSURL *fileURL;
@end
@implementation PreviewRequestFixture
@end
#endif
int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 2) return 2;
        [NSApplication sharedApplication];
#if !ZIPZI_THUMBNAIL
        Method method = class_getInstanceMethod(QLPreviewReply.class,
            @selector(initWithDataOfContentType:contentSize:dataCreationBlock:));
        OriginalReplyInit = method_setImplementation(method, (IMP)CaptureReply);
#endif
        NSString *directory = @(argv[1]);
        for (NSString *variant in @[@"image", @"absent", @"corrupt"]) {
            NSURL *url = [NSURL fileURLWithPath:[directory stringByAppendingPathComponent:
                [variant stringByAppendingString:@".zipzi"]]];
            NSImage *image = ReadImage(url);
            BOOL valid = [variant isEqualToString:@"image"];
            if ((image != nil) != valid) return 4;
            if (valid && (image.size.width != 2 || image.size.height != 1)) return 5;
#if !ZIPZI_THUMBNAIL
            ZipziPreviewProvider *provider = [[ZipziPreviewProvider alloc] init];
            PreviewRequestFixture *request = [PreviewRequestFixture new];
            request.fileURL = url;
            __block BOOL done = NO, success = NO;
            [provider providePreviewForFileRequest:(QLFilePreviewRequest *)request
                                completionHandler:^(QLPreviewReply *reply, NSError *error) {
                if (!reply || error || !CreateData) abort();
                NSError *dataError = nil;
                NSData *data = CreateData(reply, &dataError);
                success = data != nil && dataError == nil;
                if (success && ![data isEqualToData:ReadPNG(url)]) abort();
                if (!success && !dataError) abort();
                done = YES;
            }];
            NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:10];
            while (!done && deadline.timeIntervalSinceNow > 0)
                [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
            if (!done || success != valid) return 6;
#endif
        }
        puts("PASS native provider: pixels, absence and corrupt-image fallback");
        return 0;
    }
}
