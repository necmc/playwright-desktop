import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

public final class SelectionForm {
    public static void main(String[] args) {
        SwingUtilities.invokeLater(() -> showForm(args));
    }

    private static void showForm(String[] args) {
        JFrame frame = new JFrame(args[0]);
        frame.setAutoRequestFocus(false);
        frame.setDefaultCloseOperation(JFrame.DISPOSE_ON_CLOSE);

        JPanel fields = new JPanel();
        for (int i = 1; i < args.length; i++) {
            JTextField field = new JTextField(args[i], 12);
            field.getAccessibleContext().setAccessibleName("Alias");
            fields.add(field);
        }
        JButton close = new JButton("Cerrar");
        close.addActionListener(event -> frame.dispose());
        fields.add(close);

        frame.setContentPane(fields);
        frame.pack();
        frame.setLocation(-1500, 20);
        frame.setVisible(true);
    }
}
