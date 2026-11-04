// Exercise the system XPC host, separately from direct provider tests.
#import <Cocoa/Cocoa.h>
#import <QuickLookThumbnailing/QuickLookThumbnailing.h>
int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 3) return 2;
        __block BOOL done = NO;
        __block int result = 1;
        NSURL *url = [NSURL fileURLWithPath:@(argv[1])];
        QLThumbnailGenerationRequest *request = [[QLThumbnailGenerationRequest alloc]
            initWithFileAtURL:url size:CGSizeMake(256, 256) scale:1
            representationTypes:QLThumbnailGenerationRequestRepresentationTypeThumbnail];
        [[QLThumbnailGenerator sharedGenerator] generateBestRepresentationForRequest:request
            completionHandler:^(QLThumbnailRepresentation *image, NSError *error) {
                if (image) {
                    NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithCGImage:image.CGImage];
                    NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
                    if ([png writeToFile:@(argv[2]) atomically:YES]) result = 0;
                    fprintf(stderr, "System thumbnail: %ld x %ld, representation %ld\n",
                            bitmap.pixelsWide, bitmap.pixelsHigh, (long)image.type);
                } else { fprintf(stderr, "System thumbnail failed: %s\n", error.description.UTF8String); }
                done = YES;
            }];
        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:25];
        while (!done && deadline.timeIntervalSinceNow > 0)
            [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        return done ? result : 3;
    }
}
