import { Button, ModalBackdrop, ModalContainer, ModalDialog, ModalHeader, ModalBody, ModalFooter } from "@heroui/react";

interface ClearDataConfirmDialogProps {
  showClearConfirm: boolean;
  dataActionType: "open" | "export" | "clear" | null;
  setShowClearConfirm: (open: boolean) => void;
  onConfirmClearUserData: () => void;
}

export function ClearDataConfirmDialog({
  showClearConfirm,
  dataActionType,
  setShowClearConfirm,
  onConfirmClearUserData,
}: ClearDataConfirmDialogProps) {
  return (
    <ModalBackdrop
      isOpen={showClearConfirm}
      onOpenChange={(open) => {
        if (dataActionType === "clear") return;
        setShowClearConfirm(open);
      }}
    >
      <ModalContainer>
        <ModalDialog aria-label="确认清理历史数据" className="sm:max-w-[420px]">
          <ModalHeader>确认清理历史数据</ModalHeader>
          <ModalBody>
            <p className="text-sm text-foreground-500">
              将清理历史测速数据与缓存信息，仅保留 kernels 与 GeoIP。该操作不可恢复。
            </p>
          </ModalBody>
          <ModalFooter>
            <Button
              variant="secondary"
              isDisabled={dataActionType === "clear"}
              onPress={() => setShowClearConfirm(false)}
            >
              取消
            </Button>
            <Button
              variant="danger"
              isPending={dataActionType === "clear"}
              onPress={() => {
                void onConfirmClearUserData();
              }}
            >
              确认清理
            </Button>
          </ModalFooter>
        </ModalDialog>
      </ModalContainer>
    </ModalBackdrop>
  );
}

export default ClearDataConfirmDialog;
