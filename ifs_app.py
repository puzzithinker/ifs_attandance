import tkinter as tk
from tkinter import messagebox, filedialog
import sqlite3
from datetime import datetime
import csv
from urllib.parse import urlparse, parse_qs

# Function to handle the Enter key press event
def check_registration(event):
    url = entry.get()

    # Parse the URL and extract categoryCode and licenseNo
    parsed_url = urlparse(url)
    query_params = parse_qs(parsed_url.query)
    category = query_params.get('categoryCode', [''])[0]
    number = query_params.get('licenseNo', [''])[0]

    # Open a connection to the SQLite database
    conn = sqlite3.connect('agent.db')
    cursor = conn.cursor()

    # Check if the 保險中介人編號 already exists in the attendance table
    cursor.execute('''
        SELECT COUNT(*)
        FROM attendance
        WHERE 保險中介人編號 = ?
    ''', (number,))
    count = cursor.fetchone()[0]

    if count > 0:
        messagebox.showinfo("Registration Status", "已登記 (重複)")
    else:
        # Insert the data into the attendance table
        cursor.execute('''
            INSERT INTO attendance (保險中介人類別, 保險中介人編號, timestamp)
            VALUES (?, ?, ?)
        ''', (category, number, datetime.now().strftime("%Y-%m-%d %H:%M:%S")))

        conn.commit()
        messagebox.showinfo("Registration Status", "已登記")

    # Close the connection
    conn.close()

    # Clear the entry field
    entry.delete(0, tk.END)

    # Update the attendance count
    update_attendance_count()

# Create the attendance table or alter it if it exists
def create_attendance_table():
    conn = sqlite3.connect('agent.db')
    cursor = conn.cursor()

    # Check if the attendance table exists
    cursor.execute('''
        SELECT name FROM sqlite_master WHERE type='table' AND name='attendance'
    ''')
    table_exists = cursor.fetchone()

    if table_exists:
        # Alter the existing table to add the new columns if they don't exist
        cursor.execute('''
            PRAGMA table_info(attendance)
        ''')
        columns = [column[1] for column in cursor.fetchall()]

        if '保險中介人類別' not in columns:
            cursor.execute('''
                ALTER TABLE attendance ADD COLUMN 保險中介人類別 TEXT
            ''')

        if '保險中介人編號' not in columns:
            cursor.execute('''
                ALTER TABLE attendance ADD COLUMN 保險中介人編號 TEXT
            ''')

        # Update the unique constraint
        cursor.execute('''
            CREATE UNIQUE INDEX IF NOT EXISTS idx_attendance_unique
            ON attendance (保險中介人類別, 保險中介人編號)
        ''')
    else:
        # Create the attendance table with the new schema
        cursor.execute('''
            CREATE TABLE attendance (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                保險中介人類別 TEXT,
                保險中介人編號 TEXT,
                timestamp TEXT,
                UNIQUE(保險中介人類別, 保險中介人編號)
            )
        ''')

    conn.commit()
    conn.close()

# Function to export the attendance table to a CSV file
def export_attendance():
    conn = sqlite3.connect('agent.db')
    cursor = conn.cursor()

    cursor.execute("SELECT * FROM attendance")
    data = cursor.fetchall()

    default_filename = datetime.now().strftime("IFS_AML_seminar_attendance_%d-%B.csv")
    file_path = filedialog.asksaveasfilename(defaultextension=".csv", initialfile=default_filename)
    
    if file_path:
        with open(file_path, 'w', newline='', encoding='utf-8-sig') as file:
            writer = csv.writer(file)
            writer.writerow(['ID', '保險中介人類別', '保險中介人編號', 'Timestamp'])
            writer.writerows(data)

        messagebox.showinfo("Export", "Attendance data exported successfully.")

    conn.close()

# Function to update the attendance count label
def update_attendance_count():
    conn = sqlite3.connect('agent.db')
    cursor = conn.cursor()

    cursor.execute("SELECT COUNT(*) FROM attendance")
    count = cursor.fetchone()[0]

    attendance_count_label.config(text=f"已入場人數: {count}")

    conn.close()

# Create the main window
window = tk.Tk()
window.title("IFS AML Seminar Attendance Checker")
window.geometry("400x300")  # Set the window size

# Create the label and entry field for 中介人一戶通QR Code
label = tk.Label(window, text="輸入中介人一戶通QR Code:")
label.pack()
entry = tk.Entry(window)
entry.pack()
entry.focus()  # Set focus on the entry field

# Bind the Enter key press event to the check_registration function
window.bind('<Return>', check_registration)

# Create the attendance count label
attendance_count_label = tk.Label(window, text="已入場人數: 0", font=("Arial", 24, "bold"), padx=20, pady=10)
attendance_count_label.pack()

# Create the menu bar
menu_bar = tk.Menu(window)
file_menu = tk.Menu(menu_bar, tearoff=0)
file_menu.add_command(label="Export Attendance", command=export_attendance)
menu_bar.add_cascade(label="File", menu=file_menu)
window.config(menu=menu_bar)

# Create the attendance table or alter it if it exists
create_attendance_table()

# Update the initial attendance count
update_attendance_count()

# Run the GUI event loop
window.mainloop()
