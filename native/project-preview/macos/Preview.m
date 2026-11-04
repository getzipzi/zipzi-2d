#import <Cocoa/Cocoa.h>
#import <QuickLookThumbnailing/QuickLookThumbnailing.h>
#import <Quartz/Quartz.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#include "zipzi_preview.h"

static intptr_t ReadAt(void *context, uint64_t offset, uint8_t *bytes, size_t count) {
    ssize_t result;
    do { result = pread(*(int *)context, bytes, count, (off_t)offset); }
    while (result < 0 && errno == EINTR);
    return result;
}

// The Rust reader only touches the bounded index, format and embedded PNG.
// Close the project before giving any pixels to the system UI.
static NSImage *ReadImage(NSURL *url) {
    int fd = open(url.fileSystemRepresentation, O_RDONLY | O_CLOEXEC | O_NONBLOCK);
    if (fd < 0) return nil;
    struct stat info;
    NSImage *image = nil;
    if (fstat(fd, &info) == 0 && S_ISREG(info.st_mode) && info.st_size > 0) {
        NSMutableData *data = [NSMutableData dataWithLength:ZIPZI_PREVIEW_CAPACITY];
        int64_t size = zipzi_preview_read(&fd, ReadAt, (uint64_t)info.st_size,
                                         data.mutableBytes, data.length);
        if (size > 0) {
            data.length = (NSUInteger)size;
            image = [[NSImage alloc] initWithData:data];
        }
    }
    close(fd);
    return image;
}

static NSError *NoPreview(void) {
    return [NSError errorWithDomain:NSCocoaErrorDomain code:NSFileReadCorruptFileError
                          userInfo:nil];
}

#if ZIPZI_THUMBNAIL
@interface ZipziThumbnailProvider : QLThumbnailProvider
@end
@implementation ZipziThumbnailProvider
- (void)provideThumbnailForFileRequest:(QLFileThumbnailRequest *)request
                    completionHandler:(void (^)(QLThumbnailReply *, NSError *))handler {
    NSImage *image = ReadImage(request.fileURL);
    if (!image) { handler(nil, NoPreview()); return; }
    CGFloat scale = MIN(request.maximumSize.width / image.size.width,
                        request.maximumSize.height / image.size.height);
    CGSize size = CGSizeMake(MAX(1, image.size.width * scale), MAX(1, image.size.height * scale));
    handler([QLThumbnailReply replyWithContextSize:size currentContextDrawingBlock:^BOOL {
        [image drawInRect:NSMakeRect(0, 0, size.width, size.height)];
        return YES;
    }], nil);
}
@end
#else
@interface ZipziPreviewController : NSViewController <QLPreviewingController>
@end
@implementation ZipziPreviewController
- (void)loadView {
    NSImageView *image = [[NSImageView alloc] initWithFrame:NSMakeRect(0, 0, 800, 800)];
    image.imageScaling = NSImageScaleProportionallyUpOrDown;
    self.view = image;
}
- (void)preparePreviewOfFileAtURL:(NSURL *)url completionHandler:(void (^)(NSError *))handler {
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSImage *image = ReadImage(url);
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!image) { handler(NoPreview()); return; }
            ((NSImageView *)self.view).image = image;
            self.preferredContentSize = image.size;
            handler(nil);
        });
    });
}
@end
#endif
