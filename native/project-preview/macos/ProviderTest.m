// Compile twice with ZIPZI_THUMBNAIL=0/1, linked to the bounded Rust reader.
#import "Preview.m"
int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 2) return 2;
        [NSApplication sharedApplication];
        NSString *directory = @(argv[1]);
        for (NSString *variant in @[@"image", @"absent", @"corrupt"]) {
            NSURL *url = [NSURL fileURLWithPath:[directory stringByAppendingPathComponent:
                [variant stringByAppendingString:@".zipzi"]]];
            NSImage *image = ReadImage(url);
            BOOL valid = [variant isEqualToString:@"image"];
            if ((image != nil) != valid) return 4;
            if (valid && (image.size.width != 2 || image.size.height != 1)) return 5;
#if !ZIPZI_THUMBNAIL
            ZipziPreviewController *controller = [[ZipziPreviewController alloc] init];
            __block BOOL done = NO, success = NO;
            [controller preparePreviewOfFileAtURL:url completionHandler:^(NSError *error) {
                success = error == nil;
                if (success) success = ((NSImageView *)controller.view).image != nil;
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
